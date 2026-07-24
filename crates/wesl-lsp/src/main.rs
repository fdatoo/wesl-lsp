use std::{
    borrow::Cow,
    collections::{HashMap, VecDeque},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use lsp_server::{Connection, Message, Notification, Request, RequestId, Response};
use lsp_types::{
    CompletionItem, CompletionItemKind, CompletionOptions, CompletionParams, CompletionResponse,
    ConfigurationItem, ConfigurationParams, Diagnostic as LspDiagnostic,
    DiagnosticRelatedInformation, DiagnosticSeverity as LspDiagnosticSeverity,
    DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    DidSaveTextDocumentParams, DocumentFormattingParams, DocumentSymbol, DocumentSymbolParams,
    DocumentSymbolResponse, GotoDefinitionParams, GotoDefinitionResponse, Hover, HoverContents,
    HoverParams, HoverProviderCapability, InitializeParams, InitializeResult, InsertTextFormat,
    Location as LspLocation, MarkupContent, MarkupKind, OneOf, Position as LspPosition,
    PublishDiagnosticsParams, Range as LspRange, ReferenceParams, RenameOptions, RenameParams,
    ServerCapabilities, SymbolKind as LspSymbolKind, TextDocumentSyncCapability,
    TextDocumentSyncKind, TextEdit, Url, WorkDoneProgressOptions, WorkspaceEdit,
    notification::{
        DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, DidSaveTextDocument,
        Notification as NotificationTrait, PublishDiagnostics,
    },
    request::{
        Completion, DocumentSymbolRequest, Formatting, GotoDefinition, HoverRequest, References,
        Rename, Request as RequestTrait, WorkspaceConfiguration,
    },
};
use serde::Deserialize;
use wesl_analysis::{
    AnalysisHost, Completion as AnalysisCompletion, CompletionKind, DiagnosticSeverity, LineIndex,
    Symbol, SymbolKind,
};

const DEBOUNCE: Duration = Duration::from_millis(150);

#[derive(Default, Deserialize)]
struct InitializationOptions {
    root: Option<PathBuf>,
}

fn main() -> Result<()> {
    env_logger::init();
    let log_timing = std::env::args().any(|argument| argument == "--log-timing");
    let (connection, io_threads) = Connection::stdio();
    let (initialize_id, initialize_params) = connection.initialize_start()?;
    let initialize_params: InitializeParams = serde_json::from_value(initialize_params)?;
    let supports_configuration = initialize_params
        .capabilities
        .workspace
        .as_ref()
        .and_then(|workspace| workspace.configuration)
        .unwrap_or(false);
    #[allow(deprecated)]
    let configuration_scope = initialize_params
        .workspace_folders
        .as_ref()
        .and_then(|folders| folders.first())
        .map(|folder| folder.uri.clone())
        .or_else(|| initialize_params.root_uri.clone());
    let mut options: InitializationOptions = initialize_params
        .initialization_options
        .map(serde_json::from_value)
        .transpose()
        .context("invalid initializationOptions")?
        .unwrap_or_default();
    options.root = options
        .root
        .map(|root| resolve_workspace_root(root, configuration_scope.as_ref()));
    let result = InitializeResult {
        capabilities: capabilities(),
        server_info: Some(lsp_types::ServerInfo {
            name: "wesl-lsp".into(),
            version: Some(env!("CARGO_PKG_VERSION").into()),
        }),
    };
    connection.initialize_finish(initialize_id, serde_json::to_value(result)?)?;
    let mut startup_messages = VecDeque::new();
    if options.root.is_none() && supports_configuration {
        let (root, queued) = request_workspace_root(&connection, configuration_scope.clone())?;
        options.root = root;
        startup_messages = queued;
    }

    let mut server = Server {
        connection: &connection,
        analysis: AnalysisHost::new(options.root),
        versions: HashMap::new(),
        pending: HashMap::new(),
        startup_messages,
        log_timing,
    };
    server.run()?;
    drop(server);
    drop(connection);
    io_threads.join()?;
    Ok(())
}

fn request_workspace_root(
    connection: &Connection,
    scope_uri: Option<Url>,
) -> Result<(Option<PathBuf>, VecDeque<Message>)> {
    let id = RequestId::from("wesl-lsp/workspace-configuration".to_owned());
    let request = Request::new(
        id.clone(),
        WorkspaceConfiguration::METHOD.to_owned(),
        ConfigurationParams {
            items: vec![ConfigurationItem {
                scope_uri: scope_uri.clone(),
                section: Some("wesl-lsp".to_owned()),
            }],
        },
    );
    connection.sender.send(Message::Request(request))?;

    let mut queued = VecDeque::new();
    loop {
        let message = connection
            .receiver
            .recv()
            .context("client disconnected before workspace/configuration response")?;
        if let Message::Response(response) = &message
            && response.id == id
        {
            let root = response
                .result
                .as_ref()
                .and_then(|result| result.as_array())
                .and_then(|values| values.first())
                .and_then(configuration_root)
                .map(|root| resolve_workspace_root(root, scope_uri.as_ref()));
            return Ok((root, queued));
        }
        queued.push_back(message);
    }
}

fn configuration_root(value: &serde_json::Value) -> Option<PathBuf> {
    value
        .get("root")
        .or_else(|| value.get("initializationOptions")?.get("root"))
        .and_then(serde_json::Value::as_str)
        .map(PathBuf::from)
}

fn resolve_workspace_root(root: PathBuf, scope_uri: Option<&Url>) -> PathBuf {
    if root.is_absolute() {
        return root;
    }
    scope_uri
        .and_then(|uri| uri.to_file_path().ok())
        .map(|scope| scope.join(&root))
        .unwrap_or(root)
}

fn capabilities() -> ServerCapabilities {
    ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        definition_provider: Some(OneOf::Left(true)),
        references_provider: Some(OneOf::Left(true)),
        rename_provider: Some(OneOf::Right(RenameOptions {
            prepare_provider: Some(false),
            work_done_progress_options: WorkDoneProgressOptions::default(),
        })),
        document_symbol_provider: Some(OneOf::Left(true)),
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        completion_provider: Some(CompletionOptions {
            trigger_characters: Some(vec![".".to_owned()]),
            ..CompletionOptions::default()
        }),
        document_formatting_provider: Some(OneOf::Left(true)),
        ..ServerCapabilities::default()
    }
}

struct Server<'a> {
    connection: &'a Connection,
    analysis: AnalysisHost,
    versions: HashMap<PathBuf, i32>,
    pending: HashMap<PathBuf, Instant>,
    startup_messages: VecDeque<Message>,
    log_timing: bool,
}

impl Server<'_> {
    fn run(&mut self) -> Result<()> {
        loop {
            let message = if let Some(message) = self.startup_messages.pop_front() {
                Some(message)
            } else if let Some(deadline) = self.pending.values().min().copied() {
                let timeout = deadline.saturating_duration_since(Instant::now());
                match self.connection.receiver.recv_timeout(timeout) {
                    Ok(message) => Some(message),
                    Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                        log::debug!("diagnostic debounce expired");
                        None
                    }
                    Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
                }
            } else {
                match self.connection.receiver.recv() {
                    Ok(message) => Some(message),
                    Err(_) => break,
                }
            };

            if let Some(message) = message {
                match message {
                    Message::Request(request) => {
                        if self.connection.handle_shutdown(&request)? {
                            break;
                        }
                        self.handle_request(request)?;
                    }
                    Message::Notification(notification) => {
                        self.handle_notification(notification)?
                    }
                    Message::Response(_) => {}
                }
            }
            self.flush_due_diagnostics()?;
        }
        Ok(())
    }

    fn handle_notification(&mut self, notification: Notification) -> Result<()> {
        log::debug!("notification {}", notification.method);
        match notification.method.as_str() {
            DidOpenTextDocument::METHOD => {
                let params: DidOpenTextDocumentParams =
                    serde_json::from_value(notification.params)?;
                let path = uri_path(&params.text_document.uri)?;
                self.versions
                    .insert(path.clone(), params.text_document.version);
                self.analysis.open(path.clone(), params.text_document.text);
                self.publish(&path)?;
            }
            DidChangeTextDocument::METHOD => {
                let params: DidChangeTextDocumentParams =
                    serde_json::from_value(notification.params)?;
                let path = uri_path(&params.text_document.uri)?;
                if let Some(change) = params.content_changes.into_iter().last() {
                    self.versions
                        .insert(path.clone(), params.text_document.version);
                    self.analysis.change(&path, change.text);
                    self.pending.insert(path.clone(), Instant::now() + DEBOUNCE);
                    log::debug!("queued diagnostics for {}", path.display());
                }
            }
            DidSaveTextDocument::METHOD => {
                let params: DidSaveTextDocumentParams =
                    serde_json::from_value(notification.params)?;
                let path = uri_path(&params.text_document.uri)?;
                if let Some(text) = params.text {
                    self.analysis.change(&path, text);
                }
                self.pending.remove(&path);
                self.publish(&path)?;
            }
            DidCloseTextDocument::METHOD => {
                let params: DidCloseTextDocumentParams =
                    serde_json::from_value(notification.params)?;
                let path = uri_path(&params.text_document.uri)?;
                self.analysis.close(&path);
                self.pending.remove(&path);
                self.versions.remove(&path);
                self.send_diagnostics(&path, Vec::new())?;
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_request(&mut self, request: Request) -> Result<()> {
        let started = Instant::now();
        let method = request.method.clone();
        let response = match method.as_str() {
            HoverRequest::METHOD => {
                let params: HoverParams = serde_json::from_value(request.params)?;
                let path = uri_path(&params.text_document_position_params.text_document.uri)?;
                let source = self.analysis.source(&path).unwrap_or_default().to_owned();
                let result =
                    position_offset(&source, params.text_document_position_params.position)
                        .and_then(|offset| self.analysis.hover(&path, offset))
                        .map(|hover| {
                            let mut value = format!("```wesl\n{}\n```", hover.signature);
                            if let Some(documentation) = hover.documentation {
                                value.push_str("\n\n");
                                value.push_str(&documentation);
                            }
                            Hover {
                                contents: HoverContents::Markup(MarkupContent {
                                    kind: MarkupKind::Markdown,
                                    value,
                                }),
                                range: None,
                            }
                        });
                Response::new_ok(request.id, result)
            }
            Completion::METHOD => {
                let params: CompletionParams = serde_json::from_value(request.params)?;
                let path = uri_path(&params.text_document_position.text_document.uri)?;
                let source = self.analysis.source(&path).unwrap_or_default().to_owned();
                let result = position_offset(&source, params.text_document_position.position).map(
                    |offset| {
                        CompletionResponse::Array(
                            self.analysis
                                .completions(&path, offset)
                                .into_iter()
                                .filter_map(completion_item)
                                .collect(),
                        )
                    },
                );
                Response::new_ok(request.id, result)
            }
            Formatting::METHOD => {
                let params: DocumentFormattingParams = serde_json::from_value(request.params)?;
                let path = uri_path(&params.text_document.uri)?;
                let edits = self.analysis.source(&path).and_then(|source| {
                    let formatted =
                        wesl_fmt::format(source, params.options.tab_size as usize, &path)?;
                    (formatted != source).then(|| {
                        vec![TextEdit {
                            range: full_range(source),
                            new_text: formatted,
                        }]
                    })
                });
                Response::new_ok(request.id, edits)
            }
            GotoDefinition::METHOD => {
                let params: GotoDefinitionParams = serde_json::from_value(request.params)?;
                let path = uri_path(&params.text_document_position_params.text_document.uri)?;
                let offset = position_offset(
                    self.analysis.source(&path).unwrap_or_default(),
                    params.text_document_position_params.position,
                );
                let result: Option<GotoDefinitionResponse> = offset
                    .and_then(|offset| self.analysis.definition(&path, offset))
                    .and_then(lsp_location)
                    .map(GotoDefinitionResponse::Scalar);
                Response::new_ok(request.id, result)
            }
            References::METHOD => {
                let params: ReferenceParams = serde_json::from_value(request.params)?;
                let path = uri_path(&params.text_document_position.text_document.uri)?;
                let offset = position_offset(
                    self.analysis.source(&path).unwrap_or_default(),
                    params.text_document_position.position,
                );
                let result = offset
                    .map(|offset| {
                        self.analysis
                            .references(&path, offset, params.context.include_declaration)
                            .into_iter()
                            .filter_map(lsp_location)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                Response::new_ok(request.id, result)
            }
            Rename::METHOD => {
                let params: RenameParams = serde_json::from_value(request.params)?;
                let path = uri_path(&params.text_document_position.text_document.uri)?;
                let offset = position_offset(
                    self.analysis.source(&path).unwrap_or_default(),
                    params.text_document_position.position,
                );
                match offset
                    .map(|offset| self.analysis.rename(&path, offset, &params.new_name))
                    .transpose()
                {
                    Ok(edits) => {
                        let mut changes: HashMap<Url, Vec<TextEdit>> = HashMap::new();
                        for edit in edits.unwrap_or_default() {
                            let Some(range) = lsp_range_for_file(&edit.path, edit.range) else {
                                continue;
                            };
                            let Ok(uri) = Url::from_file_path(&edit.path) else {
                                continue;
                            };
                            changes.entry(uri).or_default().push(TextEdit {
                                range,
                                new_text: edit.new_text,
                            });
                        }
                        Response::new_ok(
                            request.id,
                            WorkspaceEdit {
                                changes: Some(changes),
                                document_changes: None,
                                change_annotations: None,
                            },
                        )
                    }
                    Err(message) => Response::new_err(
                        request.id,
                        lsp_server::ErrorCode::InvalidRequest as i32,
                        message.to_owned(),
                    ),
                }
            }
            DocumentSymbolRequest::METHOD => {
                let params: DocumentSymbolParams = serde_json::from_value(request.params)?;
                let path = uri_path(&params.text_document.uri)?;
                let symbols = self
                    .analysis
                    .document_symbols(&path)
                    .into_iter()
                    .filter_map(|symbol| document_symbol(&path, symbol))
                    .collect();
                Response::new_ok(request.id, DocumentSymbolResponse::Nested(symbols))
            }
            _ => Response::new_ok(request.id, serde_json::Value::Null),
        };
        if self.log_timing {
            log::info!("{} completed in {:?}", method, started.elapsed());
        }
        self.connection.sender.send(Message::Response(response))?;
        Ok(())
    }

    fn flush_due_diagnostics(&mut self) -> Result<()> {
        let now = Instant::now();
        let due: Vec<_> = self
            .pending
            .iter()
            .filter_map(|(path, deadline)| (*deadline <= now).then_some(path.clone()))
            .collect();
        for path in due {
            self.pending.remove(&path);
            self.publish(&path)?;
        }
        Ok(())
    }

    fn publish(&mut self, path: &Path) -> Result<()> {
        let started = Instant::now();
        let batch = self.analysis.diagnostic_batch(path);
        for (diagnostic_path, diagnostics) in batch {
            self.send_diagnostics(&diagnostic_path, diagnostics)?;
        }
        if self.log_timing {
            log::info!(
                "diagnostics for {} completed in {:?}",
                path.display(),
                started.elapsed()
            );
        }
        Ok(())
    }

    fn send_diagnostics(
        &self,
        path: &Path,
        diagnostics: Vec<wesl_analysis::Diagnostic>,
    ) -> Result<()> {
        let source = self
            .analysis
            .source(path)
            .map(Cow::Borrowed)
            .or_else(|| std::fs::read_to_string(path).ok().map(Cow::Owned))
            .unwrap_or_default();
        let lines = LineIndex::new(&source);
        let diagnostics = diagnostics
            .into_iter()
            .map(|diagnostic| {
                let start = lines
                    .offset_to_position(&source, diagnostic.range.start)
                    .unwrap_or(wesl_analysis::Position {
                        line: 0,
                        character: 0,
                    });
                let end = lines
                    .offset_to_position(&source, diagnostic.range.end)
                    .unwrap_or(start);
                let related_information = (!diagnostic.related.is_empty()).then(|| {
                    diagnostic
                        .related
                        .into_iter()
                        .filter_map(|(related_path, related_range, message)| {
                            let range = if related_path == path {
                                let start =
                                    lines.offset_to_position(&source, related_range.start)?;
                                let end = lines.offset_to_position(&source, related_range.end)?;
                                LspRange::new(
                                    LspPosition::new(start.line, start.character),
                                    LspPosition::new(end.line, end.character),
                                )
                            } else {
                                lsp_range_for_file(&related_path, related_range)?
                            };
                            Some(DiagnosticRelatedInformation {
                                location: LspLocation::new(
                                    Url::from_file_path(related_path).ok()?,
                                    range,
                                ),
                                message,
                            })
                        })
                        .collect()
                });
                LspDiagnostic {
                    range: LspRange::new(
                        LspPosition::new(start.line, start.character),
                        LspPosition::new(end.line, end.character),
                    ),
                    severity: Some(match diagnostic.severity {
                        DiagnosticSeverity::Error => LspDiagnosticSeverity::ERROR,
                        DiagnosticSeverity::Warning => LspDiagnosticSeverity::WARNING,
                    }),
                    source: Some("wesl-lsp".into()),
                    message: diagnostic.message,
                    related_information,
                    ..LspDiagnostic::default()
                }
            })
            .collect();
        let params = PublishDiagnosticsParams::new(
            Url::from_file_path(path).map_err(|_| anyhow::anyhow!("invalid file path"))?,
            diagnostics,
            self.versions.get(path).copied(),
        );
        self.connection
            .sender
            .send(Message::Notification(Notification::new(
                PublishDiagnostics::METHOD.into(),
                params,
            )))?;
        Ok(())
    }
}

fn completion_item(completion: AnalysisCompletion) -> Option<CompletionItem> {
    let additional_text_edits = if let Some(edit) = completion.additional_edit {
        Some(vec![TextEdit {
            range: lsp_range_for_file(&edit.path, edit.range)?,
            new_text: edit.new_text,
        }])
    } else {
        None
    };
    let is_snippet = completion.insert_text.is_some();
    Some(CompletionItem {
        label: completion.label.clone(),
        kind: Some(match completion.kind {
            CompletionKind::Function => CompletionItemKind::FUNCTION,
            CompletionKind::Struct => CompletionItemKind::STRUCT,
            CompletionKind::Field => CompletionItemKind::FIELD,
            CompletionKind::Variable => CompletionItemKind::VARIABLE,
            CompletionKind::Type => CompletionItemKind::TYPE_PARAMETER,
            CompletionKind::Keyword => CompletionItemKind::KEYWORD,
            CompletionKind::Snippet => CompletionItemKind::SNIPPET,
        }),
        detail: completion.detail,
        sort_text: Some(format!(
            "{}_{}",
            if is_snippet { "9" } else { "1" },
            completion.label
        )),
        insert_text: completion.insert_text,
        insert_text_format: is_snippet.then_some(InsertTextFormat::SNIPPET),
        additional_text_edits,
        ..CompletionItem::default()
    })
}

fn position_offset(source: &str, position: LspPosition) -> Option<usize> {
    LineIndex::new(source).position_to_offset(
        source,
        wesl_analysis::Position {
            line: position.line,
            character: position.character,
        },
    )
}

fn full_range(source: &str) -> LspRange {
    let end = LineIndex::new(source)
        .offset_to_position(source, source.len())
        .unwrap_or(wesl_analysis::Position {
            line: 0,
            character: 0,
        });
    LspRange::new(
        LspPosition::new(0, 0),
        LspPosition::new(end.line, end.character),
    )
}

fn lsp_range_for_file(path: &Path, range: std::ops::Range<usize>) -> Option<LspRange> {
    let source = std::fs::read_to_string(path).ok()?;
    let lines = LineIndex::new(&source);
    let start = lines.offset_to_position(&source, range.start)?;
    let end = lines.offset_to_position(&source, range.end)?;
    Some(LspRange::new(
        LspPosition::new(start.line, start.character),
        LspPosition::new(end.line, end.character),
    ))
}

fn lsp_location(location: wesl_analysis::Location) -> Option<LspLocation> {
    Some(LspLocation::new(
        Url::from_file_path(&location.path).ok()?,
        lsp_range_for_file(&location.path, location.range)?,
    ))
}

#[allow(deprecated)]
fn document_symbol(path: &Path, symbol: Symbol) -> Option<DocumentSymbol> {
    let range = lsp_range_for_file(path, symbol.full_range)?;
    let selection_range = lsp_range_for_file(path, symbol.range)?;
    let children = symbol
        .children
        .into_iter()
        .filter_map(|child| document_symbol(path, child))
        .collect::<Vec<_>>();
    Some(DocumentSymbol {
        name: symbol.name.to_string(),
        detail: Some(symbol.signature),
        kind: match symbol.kind {
            SymbolKind::Function => LspSymbolKind::FUNCTION,
            SymbolKind::Struct => LspSymbolKind::STRUCT,
            SymbolKind::Field => LspSymbolKind::FIELD,
            SymbolKind::Variable => LspSymbolKind::VARIABLE,
            SymbolKind::Constant => LspSymbolKind::CONSTANT,
            SymbolKind::Override => LspSymbolKind::VARIABLE,
            SymbolKind::Alias => LspSymbolKind::TYPE_PARAMETER,
        },
        tags: None,
        deprecated: None,
        range,
        selection_range,
        children: (!children.is_empty()).then_some(children),
    })
}

fn uri_path(uri: &Url) -> Result<PathBuf> {
    uri.to_file_path()
        .map_err(|_| anyhow::anyhow!("URI is not a file: {uri}"))
}
