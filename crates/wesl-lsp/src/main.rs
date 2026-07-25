use std::{
    borrow::Cow,
    collections::{HashMap, VecDeque},
    panic::{AssertUnwindSafe, catch_unwind},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use lsp_server::{Connection, ErrorCode, Message, Notification, Request, RequestId, Response};
use lsp_types::{
    CompletionItem, CompletionItemKind, CompletionOptions, CompletionParams, CompletionResponse,
    ConfigurationItem, ConfigurationParams, Diagnostic as LspDiagnostic, DiagnosticOptions,
    DiagnosticRelatedInformation, DiagnosticServerCapabilities,
    DiagnosticSeverity as LspDiagnosticSeverity, DidChangeTextDocumentParams,
    DidChangeWorkspaceFoldersParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    DidSaveTextDocumentParams, DocumentDiagnosticParams, DocumentDiagnosticReport,
    DocumentDiagnosticReportKind, DocumentDiagnosticReportResult, DocumentFormattingParams,
    DocumentHighlight, DocumentHighlightKind, DocumentHighlightParams,
    DocumentOnTypeFormattingOptions, DocumentOnTypeFormattingParams, DocumentRangeFormattingParams,
    DocumentSymbol, DocumentSymbolParams, DocumentSymbolResponse, Documentation,
    FileOperationFilter, FileOperationPattern, FileOperationPatternKind,
    FileOperationRegistrationOptions, FoldingRange as LspFoldingRange, FoldingRangeKind,
    FoldingRangeParams, FoldingRangeProviderCapability, FullDocumentDiagnosticReport,
    GotoDefinitionParams, GotoDefinitionResponse, Hover, HoverContents, HoverParams,
    HoverProviderCapability, InitializeParams, InitializeResult, InlayHint as LspInlayHint,
    InlayHintKind, InlayHintLabel, InlayHintOptions, InlayHintParams, InlayHintServerCapabilities,
    InsertTextFormat, Location as LspLocation, MarkupContent, MarkupKind, OneOf,
    ParameterInformation, ParameterLabel, Position as LspPosition, PositionEncodingKind,
    PrepareRenameResponse, PublishDiagnosticsParams, Range as LspRange, ReferenceParams,
    RelatedFullDocumentDiagnosticReport, RenameFilesParams, RenameOptions, RenameParams,
    SaveOptions, SelectionRange, SelectionRangeParams, SelectionRangeProviderCapability,
    ServerCapabilities, SignatureHelp, SignatureHelpOptions, SignatureHelpParams,
    SignatureInformation, SymbolInformation, SymbolKind as LspSymbolKind,
    TextDocumentContentChangeEvent, TextDocumentPositionParams, TextDocumentSyncCapability,
    TextDocumentSyncKind, TextDocumentSyncOptions, TextDocumentSyncSaveOptions, TextEdit, Url,
    WorkDoneProgressOptions, WorkspaceEdit, WorkspaceFileOperationsServerCapabilities,
    WorkspaceFoldersServerCapabilities, WorkspaceServerCapabilities, WorkspaceSymbolParams,
    WorkspaceSymbolResponse,
    notification::{
        DidChangeConfiguration, DidChangeTextDocument, DidChangeWorkspaceFolders,
        DidCloseTextDocument, DidOpenTextDocument, DidSaveTextDocument,
        Notification as NotificationTrait, PublishDiagnostics,
    },
    request::{
        Completion, DocumentDiagnosticRequest, DocumentHighlightRequest, DocumentSymbolRequest,
        FoldingRangeRequest, Formatting, GotoDefinition, HoverRequest, InlayHintRequest,
        OnTypeFormatting, PrepareRenameRequest, RangeFormatting, References, Rename,
        Request as RequestTrait, SelectionRangeRequest, SignatureHelpRequest, WillRenameFiles,
        WorkspaceConfiguration, WorkspaceSymbolRequest,
    },
};
use serde::Deserialize;
use wesl_analysis::{
    AnalysisHost, Completion as AnalysisCompletion, CompletionKind, DiagnosticSeverity, FoldKind,
    FoldingRange as AnalysisFoldingRange, InlayHint as AnalysisInlayHint, InlayHintConfig,
    InlayKind, LineIndex, PositionEncoding, SignatureHelp as AnalysisSignatureHelp, Symbol,
    SymbolKind, WorkspaceSymbol as AnalysisWorkspaceSymbol,
};

const DEBOUNCE: Duration = Duration::from_millis(150);

/// Settings under the `wesl-lsp` section, also accepted verbatim as `initializationOptions`.
/// Every field is optional so a partial settings object leaves the rest at its default.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Configuration {
    root: Option<PathBuf>,
    inlay_hints: InlayHintSettings,
    diagnostics: DiagnosticSettings,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct InlayHintSettings {
    enabled: bool,
    type_hints: bool,
    parameter_hints: bool,
    /// Off by default: see [`wesl_analysis::InlayHintConfig`].
    struct_layout_hints: bool,
}

impl Default for InlayHintSettings {
    fn default() -> Self {
        let hints = InlayHintConfig::default();
        Self {
            enabled: true,
            type_hints: hints.type_hints,
            parameter_hints: hints.parameter_hints,
            struct_layout_hints: hints.struct_layout_hints,
        }
    }
}

impl InlayHintSettings {
    fn to_config(self) -> InlayHintConfig {
        if !self.enabled {
            return InlayHintConfig {
                type_hints: false,
                parameter_hints: false,
                struct_layout_hints: false,
            };
        }
        InlayHintConfig {
            type_hints: self.type_hints,
            parameter_hints: self.parameter_hints,
            struct_layout_hints: self.struct_layout_hints,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct DiagnosticSettings {
    enabled: bool,
}

impl Default for DiagnosticSettings {
    fn default() -> Self {
        Self { enabled: true }
    }
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
    let mut configuration: Configuration = initialize_params
        .initialization_options
        .as_ref()
        .map(parse_configuration)
        .unwrap_or_default();
    configuration.root = configuration
        .root
        .map(|root| resolve_workspace_root(root, configuration_scope.as_ref()));
    // The analysis crate is byte-offset native, so UTF-8 removes the conversion rather than
    // swapping it for another. UTF-16 is the protocol default and the universal fallback.
    let offers_utf8 = initialize_params
        .capabilities
        .general
        .as_ref()
        .and_then(|general| general.position_encodings.as_ref())
        .is_some_and(|encodings| encodings.contains(&PositionEncodingKind::UTF8));
    let encoding = if offers_utf8 {
        PositionEncoding::Utf8
    } else {
        PositionEncoding::Utf16
    };
    let pull_diagnostics = initialize_params
        .capabilities
        .text_document
        .as_ref()
        .and_then(|text_document| text_document.diagnostic.as_ref())
        .is_some();
    let result = InitializeResult {
        capabilities: capabilities(pull_diagnostics, encoding),
        server_info: Some(lsp_types::ServerInfo {
            name: "wesl-lsp".into(),
            version: Some(env!("CARGO_PKG_VERSION").into()),
        }),
    };
    connection.initialize_finish(initialize_id, serde_json::to_value(result)?)?;
    let mut startup_messages = VecDeque::new();
    if supports_configuration {
        let (fetched, queued) = request_configuration_blocking(
            &connection,
            configuration_scope.clone(),
            configuration.root.clone(),
        )?;
        if let Some(fetched) = fetched {
            configuration = fetched;
        }
        startup_messages = queued;
    }

    let workspace_folders = initialize_params
        .workspace_folders
        .as_ref()
        .map(|folders| {
            folders
                .iter()
                .filter_map(|folder| folder.uri.to_file_path().ok())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let initial_roots = match &configuration.root {
        Some(root) => vec![root.clone()],
        None => workspace_folders.clone(),
    };

    let mut server = Server {
        connection: &connection,
        analysis: AnalysisHost::new(initial_roots),
        versions: HashMap::new(),
        pending: HashMap::new(),
        startup_messages,
        log_timing,
        push_diagnostics: !pull_diagnostics,
        configuration,
        configuration_scope,
        supports_configuration,
        workspace_folders,
        encoding,
        pending_configuration: None,
        shutting_down: false,
    };
    server.run()?;
    drop(server);
    drop(connection);
    io_threads.join()?;
    Ok(())
}

const CONFIGURATION_REQUEST_ID: &str = "wesl-lsp/workspace-configuration";

fn configuration_request(id: RequestId, scope_uri: Option<Url>) -> Request {
    Request::new(
        id,
        WorkspaceConfiguration::METHOD.to_owned(),
        ConfigurationParams {
            items: vec![ConfigurationItem {
                scope_uri,
                section: Some("wesl-lsp".to_owned()),
            }],
        },
    )
}

/// Startup-only: blocks for the response, queueing anything that arrives first so no
/// notification is lost. Runtime refreshes must not use this shape — they go through the
/// normal loop instead, see [`Server::request_configuration`].
fn request_configuration_blocking(
    connection: &Connection,
    scope_uri: Option<Url>,
    fallback_root: Option<PathBuf>,
) -> Result<(Option<Configuration>, VecDeque<Message>)> {
    let id = RequestId::from(CONFIGURATION_REQUEST_ID.to_owned());
    connection
        .sender
        .send(Message::Request(configuration_request(
            id.clone(),
            scope_uri.clone(),
        )))?;

    let mut queued = VecDeque::new();
    loop {
        let message = connection
            .receiver
            .recv()
            .context("client disconnected before workspace/configuration response")?;
        if let Message::Response(response) = &message
            && response.id == id
        {
            let configuration = response
                .result
                .as_ref()
                .map(|result| settings_from_response(result, scope_uri.as_ref(), fallback_root));
            return Ok((configuration, queued));
        }
        queued.push_back(message);
    }
}

/// Reads the first configuration item, tolerating both a bare settings object and one nested
/// under `initializationOptions`, and keeps `fallback_root` when the client supplies no root.
fn settings_from_response(
    result: &serde_json::Value,
    scope_uri: Option<&Url>,
    fallback_root: Option<PathBuf>,
) -> Configuration {
    let mut configuration = result
        .as_array()
        .and_then(|values| values.first())
        .map(parse_configuration)
        .unwrap_or_default();
    configuration.root = configuration
        .root
        .map(|root| resolve_workspace_root(root, scope_uri))
        .or(fallback_root);
    configuration
}

fn parse_configuration(value: &serde_json::Value) -> Configuration {
    let value = value.get("initializationOptions").unwrap_or(value);
    serde_json::from_value(value.clone()).unwrap_or_else(|error| {
        log::warn!("ignoring invalid wesl-lsp settings: {error}");
        Configuration::default()
    })
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

/// Diagnostics are delivered by exactly one mechanism per session. Clients that declare pull
/// support get `textDocument/diagnostic` and no pushes; everyone else — Zed among them — keeps
/// the push path. Advertising both would leave clients that do both double-reporting, and the
/// specification advises against mixing them.
fn capabilities(pull_diagnostics: bool, encoding: PositionEncoding) -> ServerCapabilities {
    ServerCapabilities {
        position_encoding: Some(match encoding {
            PositionEncoding::Utf8 => PositionEncodingKind::UTF8,
            PositionEncoding::Utf16 => PositionEncodingKind::UTF16,
        }),
        diagnostic_provider: pull_diagnostics.then(|| {
            DiagnosticServerCapabilities::Options(DiagnosticOptions {
                identifier: Some("wesl-lsp".to_owned()),
                // A shader's diagnostics depend on the files it imports, so a change elsewhere
                // in the package can change this document's result.
                inter_file_dependencies: true,
                workspace_diagnostics: false,
                work_done_progress_options: WorkDoneProgressOptions::default(),
            })
        }),
        // `include_text` on save is deliberate: it gives an authoritative full resync on every
        // save, which bounds how long an incremental change we failed to apply can persist.
        text_document_sync: Some(TextDocumentSyncCapability::Options(
            TextDocumentSyncOptions {
                open_close: Some(true),
                change: Some(TextDocumentSyncKind::INCREMENTAL),
                save: Some(TextDocumentSyncSaveOptions::SaveOptions(SaveOptions {
                    include_text: Some(true),
                })),
                ..TextDocumentSyncOptions::default()
            },
        )),
        definition_provider: Some(OneOf::Left(true)),
        references_provider: Some(OneOf::Left(true)),
        rename_provider: Some(OneOf::Right(RenameOptions {
            prepare_provider: Some(true),
            work_done_progress_options: WorkDoneProgressOptions::default(),
        })),
        document_symbol_provider: Some(OneOf::Left(true)),
        document_highlight_provider: Some(OneOf::Left(true)),
        workspace_symbol_provider: Some(OneOf::Left(true)),
        folding_range_provider: Some(FoldingRangeProviderCapability::Simple(true)),
        selection_range_provider: Some(SelectionRangeProviderCapability::Simple(true)),
        inlay_hint_provider: Some(OneOf::Right(InlayHintServerCapabilities::Options(
            InlayHintOptions::default(),
        ))),
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        completion_provider: Some(CompletionOptions {
            trigger_characters: Some(vec![".".to_owned()]),
            ..CompletionOptions::default()
        }),
        signature_help_provider: Some(SignatureHelpOptions {
            trigger_characters: Some(vec!["(".to_owned()]),
            retrigger_characters: Some(vec![",".to_owned()]),
            work_done_progress_options: WorkDoneProgressOptions::default(),
        }),
        document_formatting_provider: Some(OneOf::Left(true)),
        document_range_formatting_provider: Some(OneOf::Left(true)),
        // Newline covers "I just pressed enter"; `}` covers closing a block.
        document_on_type_formatting_provider: Some(DocumentOnTypeFormattingOptions {
            first_trigger_character: "}".to_owned(),
            more_trigger_character: Some(vec!["\n".to_owned()]),
        }),
        workspace: Some(WorkspaceServerCapabilities {
            workspace_folders: Some(WorkspaceFoldersServerCapabilities {
                supported: Some(true),
                change_notifications: Some(OneOf::Left(true)),
            }),
            file_operations: Some(WorkspaceFileOperationsServerCapabilities {
                will_rename: Some(FileOperationRegistrationOptions {
                    filters: vec![
                        FileOperationFilter {
                            scheme: Some("file".to_owned()),
                            pattern: FileOperationPattern {
                                glob: "**/*.{wesl,wgsl}".to_owned(),
                                matches: Some(FileOperationPatternKind::File),
                                options: None,
                            },
                        },
                        // Moving a directory changes the module path of every shader under
                        // it, so imports into it need the same treatment.
                        FileOperationFilter {
                            scheme: Some("file".to_owned()),
                            pattern: FileOperationPattern {
                                glob: "**".to_owned(),
                                matches: Some(FileOperationPatternKind::Folder),
                                options: None,
                            },
                        },
                    ],
                }),
                ..WorkspaceFileOperationsServerCapabilities::default()
            }),
        }),
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
    /// False when the client pulls instead; see [`capabilities`].
    push_diagnostics: bool,
    configuration: Configuration,
    configuration_scope: Option<Url>,
    supports_configuration: bool,
    /// Roots reported by the client, used only when no explicit root is configured.
    workspace_folders: Vec<PathBuf>,
    encoding: PositionEncoding,
    /// Set while a runtime settings refresh is in flight, so its response is recognised in
    /// the main loop rather than blocking for it.
    pending_configuration: Option<RequestId>,
    /// True between `shutdown` and `exit`, during which further requests are refused.
    shutting_down: bool,
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
                    // Shutdown is handled here rather than via `Connection::handle_shutdown`,
                    // which blocks for `exit` and errors on anything else that arrives. The
                    // specification instead requires post-shutdown requests to be refused with
                    // InvalidRequest while the server stays up until `exit`.
                    Message::Request(request) if request.method == "shutdown" => {
                        self.shutting_down = true;
                        self.connection
                            .sender
                            .send(Message::Response(Response::new_ok(request.id, ())))?;
                    }
                    Message::Request(request) if self.shutting_down => {
                        self.connection
                            .sender
                            .send(Message::Response(Response::new_err(
                                request.id,
                                ErrorCode::InvalidRequest as i32,
                                "server is shutting down".to_owned(),
                            )))?;
                    }
                    Message::Request(request) => {
                        self.handle_request(request)?;
                    }
                    Message::Notification(notification) if notification.method == "exit" => break,
                    Message::Notification(notification) => {
                        let method = notification.method.clone();
                        match catch_unwind(AssertUnwindSafe(|| {
                            self.handle_notification(notification)
                        })) {
                            Ok(Ok(())) => {}
                            Ok(Err(error)) => log::warn!("notification {method} failed: {error}"),
                            Err(_) => log::error!("notification {method} panicked"),
                        }
                    }
                    Message::Response(response) => {
                        if self.pending_configuration.as_ref() == Some(&response.id) {
                            self.pending_configuration = None;
                            if let Some(result) = &response.result {
                                self.apply_configuration(settings_from_response(
                                    result,
                                    self.configuration_scope.as_ref(),
                                    self.configuration.root.clone(),
                                ))?;
                            }
                        }
                    }
                }
            }
            self.flush_due_diagnostics()?;
        }
        Ok(())
    }

    /// Asks the client for settings without blocking; the answer is picked up in [`Self::run`].
    fn request_configuration(&mut self) -> Result<()> {
        if !self.supports_configuration || self.pending_configuration.is_some() {
            return Ok(());
        }
        let id = RequestId::from(CONFIGURATION_REQUEST_ID.to_owned());
        self.pending_configuration = Some(id.clone());
        self.connection
            .sender
            .send(Message::Request(configuration_request(
                id,
                self.configuration_scope.clone(),
            )))?;
        Ok(())
    }

    /// An explicit `root` setting overrides everything; otherwise the client's workspace
    /// folders are the roots, and with neither we fall back to per-file discovery.
    fn roots(&self) -> Vec<PathBuf> {
        match &self.configuration.root {
            Some(root) => vec![root.clone()],
            None => self.workspace_folders.clone(),
        }
    }

    fn apply_configuration(&mut self, configuration: Configuration) -> Result<()> {
        let root_changed = configuration.root != self.configuration.root;
        self.configuration = configuration;
        if root_changed {
            // Discards every cached package, so the next request reindexes under the new root.
            self.analysis.set_roots(self.roots());
        }
        // Diagnostics may have been switched on or off, and a new root changes what resolves.
        for path in self.versions.keys().cloned().collect::<Vec<_>>() {
            self.publish(&path)?;
        }
        Ok(())
    }

    fn handle_notification(&mut self, notification: Notification) -> Result<()> {
        log::debug!("notification {}", notification.method);
        match notification.method.as_str() {
            DidChangeConfiguration::METHOD => self.request_configuration()?,
            DidChangeWorkspaceFolders::METHOD => {
                let params: DidChangeWorkspaceFoldersParams =
                    serde_json::from_value(notification.params)?;
                let removed = params
                    .event
                    .removed
                    .iter()
                    .filter_map(|folder| folder.uri.to_file_path().ok())
                    .collect::<Vec<_>>();
                self.workspace_folders
                    .retain(|folder| !removed.contains(folder));
                self.workspace_folders.extend(
                    params
                        .event
                        .added
                        .iter()
                        .filter_map(|folder| folder.uri.to_file_path().ok()),
                );
                self.analysis.set_roots(self.roots());
                for path in self.versions.keys().cloned().collect::<Vec<_>>() {
                    self.publish(&path)?;
                }
            }
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
                if params.content_changes.is_empty() {
                    return Ok(());
                }
                let before = self.analysis.source(&path).unwrap_or_default();
                let (text, rejected) =
                    apply_content_changes(before, &params.content_changes, self.encoding);
                if rejected > 0 {
                    log::warn!(
                        "ignored {rejected} out-of-range change(s) for {}; the buffer may be \
                         stale until the next save",
                        path.display()
                    );
                }
                self.versions
                    .insert(path.clone(), params.text_document.version);
                self.analysis.change(&path, text);
                self.pending.insert(path.clone(), Instant::now() + DEBOUNCE);
                log::debug!("queued diagnostics for {}", path.display());
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

    /// One request must never take the whole server down. A handler that panics, or that
    /// rejects malformed params, fails that request alone — the editor keeps language support
    /// for everything else instead of silently losing it until restart.
    fn handle_request(&mut self, request: Request) -> Result<()> {
        let started = Instant::now();
        let id = request.id.clone();
        let method = request.method.clone();

        let dispatched = catch_unwind(AssertUnwindSafe(|| self.dispatch(request)));
        let response = match dispatched {
            Ok(Ok(response)) => response,
            Ok(Err(error)) => {
                log::warn!("{method} failed: {error}");
                Response::new_err(id, ErrorCode::InvalidParams as i32, error.to_string())
            }
            Err(_) => {
                log::error!("{method} panicked");
                Response::new_err(
                    id,
                    ErrorCode::InternalError as i32,
                    format!("internal error handling {method}"),
                )
            }
        };
        if self.log_timing {
            log::info!("{} completed in {:?}", method, started.elapsed());
        }
        self.connection.sender.send(Message::Response(response))?;
        Ok(())
    }

    fn dispatch(&mut self, request: Request) -> Result<Response> {
        let method = request.method.clone();
        Ok(match method.as_str() {
            HoverRequest::METHOD => {
                let params: HoverParams = serde_json::from_value(request.params)?;
                let path = uri_path(&params.text_document_position_params.text_document.uri)?;
                let source = self.analysis.source(&path).unwrap_or_default().to_owned();
                let result = position_offset(
                    &source,
                    params.text_document_position_params.position,
                    self.encoding,
                )
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
                let result = position_offset(
                    &source,
                    params.text_document_position.position,
                    self.encoding,
                )
                .map(|offset| {
                    CompletionResponse::Array(
                        self.analysis
                            .completions(&path, offset)
                            .into_iter()
                            .filter_map(|item| completion_item(item, self.encoding))
                            .collect(),
                    )
                });
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
                            range: full_range(source, self.encoding),
                            new_text: formatted,
                        }]
                    })
                });
                Response::new_ok(request.id, edits)
            }
            OnTypeFormatting::METHOD => {
                let params: DocumentOnTypeFormattingParams =
                    serde_json::from_value(request.params)?;
                let path = uri_path(&params.text_document_position.text_document.uri)?;
                let source = self.analysis.source(&path).unwrap_or_default().to_owned();
                let lines = LineIndex::new(&source, self.encoding);
                let result = position_offset(
                    &source,
                    params.text_document_position.position,
                    self.encoding,
                )
                .and_then(|offset| {
                    let (range, indent) = wesl_analysis::reindent_line(
                        &source,
                        offset,
                        params.options.tab_size as usize,
                    )?;
                    let from = lines.offset_to_position(&source, range.start)?;
                    let to = lines.offset_to_position(&source, range.end)?;
                    Some(vec![TextEdit {
                        range: LspRange::new(
                            LspPosition::new(from.line, from.character),
                            LspPosition::new(to.line, to.character),
                        ),
                        new_text: indent,
                    }])
                });
                Response::new_ok(request.id, result)
            }
            RangeFormatting::METHOD => {
                let params: DocumentRangeFormattingParams = serde_json::from_value(request.params)?;
                let path = uri_path(&params.text_document.uri)?;
                let source = self.analysis.source(&path).unwrap_or_default().to_owned();
                let selection = position_offset(&source, params.range.start, self.encoding)
                    .zip(position_offset(&source, params.range.end, self.encoding));
                // The whole document goes through the formatter's refuse-when-unsure gate;
                // the range only decides which of the resulting hunks are handed back.
                let edits = selection.and_then(|(start, end)| {
                    let formatted =
                        wesl_fmt::format(&source, params.options.tab_size as usize, &path)?;
                    let lines = LineIndex::new(&source, self.encoding);
                    let edits = wesl_fmt::line_hunks(&source, &formatted)
                        .into_iter()
                        .filter(|hunk| hunk.range.start <= end && hunk.range.end >= start)
                        .filter_map(|hunk| {
                            let from = lines.offset_to_position(&source, hunk.range.start)?;
                            let to = lines.offset_to_position(&source, hunk.range.end)?;
                            Some(TextEdit {
                                range: LspRange::new(
                                    LspPosition::new(from.line, from.character),
                                    LspPosition::new(to.line, to.character),
                                ),
                                new_text: hunk.new_text,
                            })
                        })
                        .collect::<Vec<_>>();
                    (!edits.is_empty()).then_some(edits)
                });
                Response::new_ok(request.id, edits)
            }
            GotoDefinition::METHOD => {
                let params: GotoDefinitionParams = serde_json::from_value(request.params)?;
                let path = uri_path(&params.text_document_position_params.text_document.uri)?;
                let offset = position_offset(
                    self.analysis.source(&path).unwrap_or_default(),
                    params.text_document_position_params.position,
                    self.encoding,
                );
                let result: Option<GotoDefinitionResponse> = offset
                    .and_then(|offset| self.analysis.definition(&path, offset))
                    .and_then(|location| lsp_location(location, self.encoding))
                    .map(GotoDefinitionResponse::Scalar);
                Response::new_ok(request.id, result)
            }
            References::METHOD => {
                let params: ReferenceParams = serde_json::from_value(request.params)?;
                let path = uri_path(&params.text_document_position.text_document.uri)?;
                let offset = position_offset(
                    self.analysis.source(&path).unwrap_or_default(),
                    params.text_document_position.position,
                    self.encoding,
                );
                let result = offset
                    .map(|offset| {
                        self.analysis
                            .references(&path, offset, params.context.include_declaration)
                            .into_iter()
                            .filter_map(|location| lsp_location(location, self.encoding))
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
                    self.encoding,
                );
                match offset
                    .map(|offset| self.analysis.rename(&path, offset, &params.new_name))
                    .transpose()
                {
                    Ok(edits) => {
                        let mut changes: HashMap<Url, Vec<TextEdit>> = HashMap::new();
                        for edit in edits.unwrap_or_default() {
                            let Some(range) =
                                lsp_range_for_file(&edit.path, edit.range, self.encoding)
                            else {
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
            WillRenameFiles::METHOD => {
                let params: RenameFilesParams = serde_json::from_value(request.params)?;
                let mut changes: HashMap<Url, Vec<TextEdit>> = HashMap::new();
                for rename in &params.files {
                    let (Some(old_path), Some(new_path)) = (
                        file_uri_path(&rename.old_uri),
                        file_uri_path(&rename.new_uri),
                    ) else {
                        continue;
                    };
                    let old_path = old_path.canonicalize().unwrap_or(old_path);
                    for edit in self.analysis.file_rename_edits(&old_path, &new_path) {
                        let Some(range) = lsp_range_for_file(&edit.path, edit.range, self.encoding)
                        else {
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
                }
                let result = (!changes.is_empty()).then_some(WorkspaceEdit {
                    changes: Some(changes),
                    document_changes: None,
                    change_annotations: None,
                });
                Response::new_ok(request.id, result)
            }
            DocumentDiagnosticRequest::METHOD => {
                let params: DocumentDiagnosticParams = serde_json::from_value(request.params)?;
                let path = uri_path(&params.text_document.uri)?;
                let mut batch = self.analysis.diagnostic_batch(&path);
                let items = batch
                    .iter()
                    .position(|(reported, _)| *reported == path)
                    .map(|index| batch.remove(index).1)
                    .unwrap_or_default();
                // The rest of the import closure rides along, so fixing a dependency clears
                // the stale squiggles in its dependents without waiting for them to be pulled.
                let related_documents = batch
                    .into_iter()
                    .filter_map(|(other, diagnostics)| {
                        let items = self.lsp_diagnostics(&other, diagnostics);
                        Some((
                            Url::from_file_path(other).ok()?,
                            DocumentDiagnosticReportKind::Full(FullDocumentDiagnosticReport {
                                result_id: None,
                                items,
                            }),
                        ))
                    })
                    .collect::<HashMap<_, _>>();
                let report = RelatedFullDocumentDiagnosticReport {
                    related_documents: (!related_documents.is_empty()).then_some(related_documents),
                    full_document_diagnostic_report: FullDocumentDiagnosticReport {
                        result_id: None,
                        items: self.lsp_diagnostics(&path, items),
                    },
                };
                Response::new_ok(
                    request.id,
                    DocumentDiagnosticReportResult::Report(DocumentDiagnosticReport::Full(report)),
                )
            }
            InlayHintRequest::METHOD => {
                let params: InlayHintParams = serde_json::from_value(request.params)?;
                let path = uri_path(&params.text_document.uri)?;
                let source = self.analysis.source(&path).unwrap_or_default().to_owned();
                let lines = LineIndex::new(&source, self.encoding);
                let start =
                    position_offset(&source, params.range.start, self.encoding).unwrap_or(0);
                let end = position_offset(&source, params.range.end, self.encoding)
                    .unwrap_or(source.len());
                let hints = self.configuration.inlay_hints.to_config();
                let result = self
                    .analysis
                    .inlay_hints(&path, start..end, hints)
                    .into_iter()
                    .filter_map(|hint| lsp_inlay_hint(&source, &lines, hint))
                    .collect::<Vec<_>>();
                Response::new_ok(request.id, result)
            }
            SignatureHelpRequest::METHOD => {
                let params: SignatureHelpParams = serde_json::from_value(request.params)?;
                let path = uri_path(&params.text_document_position_params.text_document.uri)?;
                let source = self.analysis.source(&path).unwrap_or_default().to_owned();
                let result = position_offset(
                    &source,
                    params.text_document_position_params.position,
                    self.encoding,
                )
                .and_then(|offset| self.analysis.signature_help(&path, offset))
                .map(lsp_signature_help);
                Response::new_ok(request.id, result)
            }
            SelectionRangeRequest::METHOD => {
                let params: SelectionRangeParams = serde_json::from_value(request.params)?;
                let path = uri_path(&params.text_document.uri)?;
                let source = self.analysis.source(&path).unwrap_or_default().to_owned();
                let lines = LineIndex::new(&source, self.encoding);
                let result = params
                    .positions
                    .into_iter()
                    .map(|position| {
                        position_offset(&source, position, self.encoding)
                            .map(|offset| self.analysis.selection_ranges(&path, offset))
                            .and_then(|ranges| lsp_selection_range(&source, &lines, ranges))
                            .unwrap_or_else(|| SelectionRange {
                                range: LspRange::new(position, position),
                                parent: None,
                            })
                    })
                    .collect::<Vec<_>>();
                Response::new_ok(request.id, result)
            }
            FoldingRangeRequest::METHOD => {
                let params: FoldingRangeParams = serde_json::from_value(request.params)?;
                let path = uri_path(&params.text_document.uri)?;
                let source = self.analysis.source(&path).unwrap_or_default().to_owned();
                let lines = LineIndex::new(&source, self.encoding);
                let result = self
                    .analysis
                    .folding_ranges(&path)
                    .into_iter()
                    .filter_map(|folding| lsp_folding_range(&source, &lines, folding))
                    .collect::<Vec<_>>();
                Response::new_ok(request.id, result)
            }
            WorkspaceSymbolRequest::METHOD => {
                let params: WorkspaceSymbolParams = serde_json::from_value(request.params)?;
                let symbols = self
                    .analysis
                    .workspace_symbols(&params.query)
                    .into_iter()
                    .filter_map(|found| workspace_symbol_information(found, self.encoding))
                    .collect();
                Response::new_ok(request.id, WorkspaceSymbolResponse::Flat(symbols))
            }
            DocumentHighlightRequest::METHOD => {
                let params: DocumentHighlightParams = serde_json::from_value(request.params)?;
                let path = uri_path(&params.text_document_position_params.text_document.uri)?;
                let source = self.analysis.source(&path).unwrap_or_default().to_owned();
                let result = position_offset(
                    &source,
                    params.text_document_position_params.position,
                    self.encoding,
                )
                .map(|offset| {
                    let lines = LineIndex::new(&source, self.encoding);
                    self.analysis
                        .document_highlights(&path, offset)
                        .into_iter()
                        .filter_map(|range| {
                            let start = lines.offset_to_position(&source, range.start)?;
                            let end = lines.offset_to_position(&source, range.end)?;
                            Some(DocumentHighlight {
                                range: LspRange::new(
                                    LspPosition::new(start.line, start.character),
                                    LspPosition::new(end.line, end.character),
                                ),
                                kind: Some(DocumentHighlightKind::TEXT),
                            })
                        })
                        .collect::<Vec<_>>()
                });
                Response::new_ok(request.id, result)
            }
            PrepareRenameRequest::METHOD => {
                let params: TextDocumentPositionParams = serde_json::from_value(request.params)?;
                let path = uri_path(&params.text_document.uri)?;
                let source = self.analysis.source(&path).unwrap_or_default().to_owned();
                match position_offset(&source, params.position, self.encoding)
                    .map(|offset| self.analysis.prepare_rename(&path, offset))
                    .transpose()
                {
                    Ok(range) => {
                        let lines = LineIndex::new(&source, self.encoding);
                        let result = range.and_then(|range| {
                            let start = lines.offset_to_position(&source, range.start)?;
                            let end = lines.offset_to_position(&source, range.end)?;
                            Some(PrepareRenameResponse::Range(LspRange::new(
                                LspPosition::new(start.line, start.character),
                                LspPosition::new(end.line, end.character),
                            )))
                        });
                        Response::new_ok(request.id, result)
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
                    .filter_map(|symbol| document_symbol(&path, symbol, self.encoding))
                    .collect();
                Response::new_ok(request.id, DocumentSymbolResponse::Nested(symbols))
            }
            _ => Response::new_err(
                request.id,
                ErrorCode::MethodNotFound as i32,
                format!("unsupported method {method}"),
            ),
        })
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
        if !self.push_diagnostics {
            return Ok(());
        }
        // Publish an empty set rather than skipping, so turning diagnostics off clears what
        // is already on screen instead of freezing it.
        if !self.configuration.diagnostics.enabled {
            return self.send_diagnostics(path, Vec::new());
        }
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
        let params = PublishDiagnosticsParams::new(
            Url::from_file_path(path).map_err(|_| anyhow::anyhow!("invalid file path"))?,
            self.lsp_diagnostics(path, diagnostics),
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

    fn lsp_diagnostics(
        &self,
        path: &Path,
        diagnostics: Vec<wesl_analysis::Diagnostic>,
    ) -> Vec<LspDiagnostic> {
        let source = self
            .analysis
            .source(path)
            .map(Cow::Borrowed)
            .or_else(|| std::fs::read_to_string(path).ok().map(Cow::Owned))
            .unwrap_or_default();
        let lines = LineIndex::new(&source, self.encoding);
        diagnostics
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
                                lsp_range_for_file(&related_path, related_range, self.encoding)?
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
            .collect()
    }
}

fn completion_item(
    completion: AnalysisCompletion,
    encoding: PositionEncoding,
) -> Option<CompletionItem> {
    let additional_text_edits = if let Some(edit) = completion.additional_edit {
        Some(vec![TextEdit {
            range: lsp_range_for_file(&edit.path, edit.range, encoding)?,
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

/// Applies content changes in order, returning the new text and how many were rejected.
///
/// A ranged change is incremental, so both endpoints resolve against the text as it stands
/// immediately before that change — not against the original. Getting this wrong desynchronises
/// the server's buffer from the editor's silently: no error is raised, every later answer is
/// computed against the wrong text, and only a save resynchronises. An out-of-range change is
/// skipped rather than guessed at, for the same reason.
fn apply_content_changes(
    before: &str,
    changes: &[TextDocumentContentChangeEvent],
    encoding: PositionEncoding,
) -> (String, usize) {
    let mut text = before.to_owned();
    let mut rejected = 0;
    for change in changes {
        match change.range {
            Some(range) => {
                let start = position_offset(&text, range.start, encoding);
                let end = position_offset(&text, range.end, encoding);
                match (start, end) {
                    (Some(start), Some(end)) if start <= end => {
                        text.replace_range(start..end, &change.text);
                    }
                    _ => rejected += 1,
                }
            }
            None => text = change.text.clone(),
        }
    }
    (text, rejected)
}

fn position_offset(
    source: &str,
    position: LspPosition,
    encoding: PositionEncoding,
) -> Option<usize> {
    LineIndex::new(source, encoding).position_to_offset(
        source,
        wesl_analysis::Position {
            line: position.line,
            character: position.character,
        },
    )
}

fn full_range(source: &str, encoding: PositionEncoding) -> LspRange {
    let end = LineIndex::new(source, encoding)
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

fn lsp_range_for_file(
    path: &Path,
    range: std::ops::Range<usize>,
    encoding: PositionEncoding,
) -> Option<LspRange> {
    let source = std::fs::read_to_string(path).ok()?;
    let lines = LineIndex::new(&source, encoding);
    let start = lines.offset_to_position(&source, range.start)?;
    let end = lines.offset_to_position(&source, range.end)?;
    Some(LspRange::new(
        LspPosition::new(start.line, start.character),
        LspPosition::new(end.line, end.character),
    ))
}

fn lsp_location(
    location: wesl_analysis::Location,
    encoding: PositionEncoding,
) -> Option<LspLocation> {
    Some(LspLocation::new(
        Url::from_file_path(&location.path).ok()?,
        lsp_range_for_file(&location.path, location.range, encoding)?,
    ))
}

/// Type hints render after the name and parameter hints before the argument, so each side
/// gets the padding that reads naturally: `let x: f32` and `clamp(low: 0.0)`.
fn lsp_inlay_hint(
    source: &str,
    lines: &LineIndex,
    hint: AnalysisInlayHint,
) -> Option<LspInlayHint> {
    let position = lines.offset_to_position(source, hint.offset)?;
    // Layout hints are neither a type nor a parameter, so they carry no kind and clients
    // render them in the neutral inlay style.
    let (kind, pad_left, pad_right) = match hint.kind {
        InlayKind::Type => (Some(InlayHintKind::TYPE), false, false),
        InlayKind::Parameter => (Some(InlayHintKind::PARAMETER), false, true),
        InlayKind::Layout => (None, true, false),
    };
    Some(LspInlayHint {
        position: LspPosition::new(position.line, position.character),
        label: InlayHintLabel::String(hint.label),
        kind,
        text_edits: None,
        tooltip: None,
        padding_left: Some(pad_left),
        padding_right: Some(pad_right),
        data: None,
    })
}

/// Parameter labels are sent as offsets into the signature label rather than as substrings,
/// so a client highlights the right occurrence when two parameters share a spelling.
fn lsp_signature_help(help: AnalysisSignatureHelp) -> SignatureHelp {
    SignatureHelp {
        signatures: help
            .signatures
            .into_iter()
            .map(|signature| SignatureInformation {
                parameters: Some(
                    signature
                        .parameters
                        .iter()
                        .map(|span| ParameterInformation {
                            label: ParameterLabel::LabelOffsets([
                                utf16_length(&signature.label[..span.start]),
                                utf16_length(&signature.label[..span.end]),
                            ]),
                            documentation: None,
                        })
                        .collect(),
                ),
                documentation: signature.documentation.map(|value| {
                    Documentation::MarkupContent(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value,
                    })
                }),
                label: signature.label,
                active_parameter: None,
            })
            .collect(),
        active_signature: Some(help.active_signature as u32),
        active_parameter: Some(help.active_parameter as u32),
    }
}

fn utf16_length(text: &str) -> u32 {
    text.encode_utf16().count() as u32
}

/// Folds the innermost-first chain into the protocol's outermost-rooted linked list.
fn lsp_selection_range(
    source: &str,
    lines: &LineIndex,
    ranges: Vec<std::ops::Range<usize>>,
) -> Option<SelectionRange> {
    let mut parent: Option<Box<SelectionRange>> = None;
    for range in ranges.into_iter().rev() {
        let start = lines.offset_to_position(source, range.start)?;
        let end = lines.offset_to_position(source, range.end)?;
        parent = Some(Box::new(SelectionRange {
            range: LspRange::new(
                LspPosition::new(start.line, start.character),
                LspPosition::new(end.line, end.character),
            ),
            parent,
        }));
    }
    parent.map(|innermost| *innermost)
}

/// Brace regions keep their closing line visible, because collapsing a function should still
/// show the `}` that ends it. Comment and import runs have no such delimiter, so the whole run
/// collapses.
fn lsp_folding_range(
    source: &str,
    lines: &LineIndex,
    folding: AnalysisFoldingRange,
) -> Option<LspFoldingRange> {
    let start = lines.offset_to_position(source, folding.range.start)?;
    let end = lines.offset_to_position(source, folding.range.end)?;
    let end_line = match folding.kind {
        FoldKind::Region => end.line.checked_sub(1)?,
        FoldKind::Comment | FoldKind::Imports => end.line,
    };
    (end_line > start.line).then(|| LspFoldingRange {
        start_line: start.line,
        end_line,
        kind: Some(match folding.kind {
            FoldKind::Region => FoldingRangeKind::Region,
            FoldKind::Comment => FoldingRangeKind::Comment,
            FoldKind::Imports => FoldingRangeKind::Imports,
        }),
        ..LspFoldingRange::default()
    })
}

/// `SymbolInformation` is deprecated in favour of the 3.17 nested `WorkspaceSymbol`, but it
/// is what every client understands, so the flat shape is the compatible choice here.
#[allow(deprecated)]
fn workspace_symbol_information(
    found: AnalysisWorkspaceSymbol,
    encoding: PositionEncoding,
) -> Option<SymbolInformation> {
    let symbol = found.symbol;
    Some(SymbolInformation {
        name: symbol.name.to_string(),
        kind: lsp_symbol_kind(symbol.kind),
        tags: None,
        deprecated: None,
        location: LspLocation::new(
            Url::from_file_path(&symbol.path).ok()?,
            lsp_range_for_file(&symbol.path, symbol.range, encoding)?,
        ),
        container_name: found.container,
    })
}

fn lsp_symbol_kind(kind: SymbolKind) -> LspSymbolKind {
    match kind {
        SymbolKind::Function => LspSymbolKind::FUNCTION,
        SymbolKind::Struct => LspSymbolKind::STRUCT,
        SymbolKind::Field => LspSymbolKind::FIELD,
        SymbolKind::Variable => LspSymbolKind::VARIABLE,
        SymbolKind::Constant => LspSymbolKind::CONSTANT,
        SymbolKind::Override => LspSymbolKind::VARIABLE,
        SymbolKind::Alias => LspSymbolKind::TYPE_PARAMETER,
    }
}

#[allow(deprecated)]
fn document_symbol(
    path: &Path,
    symbol: Symbol,
    encoding: PositionEncoding,
) -> Option<DocumentSymbol> {
    let range = lsp_range_for_file(path, symbol.full_range, encoding)?;
    let selection_range = lsp_range_for_file(path, symbol.range, encoding)?;
    let children = symbol
        .children
        .into_iter()
        .filter_map(|child| document_symbol(path, child, encoding))
        .collect::<Vec<_>>();
    Some(DocumentSymbol {
        name: symbol.name.to_string(),
        detail: Some(symbol.signature),
        kind: lsp_symbol_kind(symbol.kind),
        tags: None,
        deprecated: None,
        range,
        selection_range,
        children: (!children.is_empty()).then_some(children),
    })
}

/// File-operation params carry URIs as plain strings rather than parsed `Url`s.
fn file_uri_path(uri: &str) -> Option<PathBuf> {
    Url::parse(uri).ok()?.to_file_path().ok()
}

fn uri_path(uri: &Url) -> Result<PathBuf> {
    let path = uri
        .to_file_path()
        .map_err(|_| anyhow::anyhow!("URI is not a file: {uri}"))?;
    Ok(path.canonicalize().unwrap_or(path))
}

#[cfg(test)]
mod tests {
    use super::{apply_content_changes, position_offset};
    use lsp_types::{Position as LspPosition, Range as LspRange, TextDocumentContentChangeEvent};
    use wesl_analysis::{LineIndex, PositionEncoding};

    /// Deterministic so a failure reproduces exactly from the seed printed in the assertion.
    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            // xorshift64*
            self.0 ^= self.0 >> 12;
            self.0 ^= self.0 << 25;
            self.0 ^= self.0 >> 27;
            self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
        }

        fn below(&mut self, bound: usize) -> usize {
            if bound == 0 {
                0
            } else {
                (self.next() % bound as u64) as usize
            }
        }
    }

    fn change(
        source: &str,
        range: std::ops::Range<usize>,
        text: &str,
        encoding: PositionEncoding,
    ) -> TextDocumentContentChangeEvent {
        let lines = LineIndex::new(source, encoding);
        let start = lines.offset_to_position(source, range.start).unwrap();
        let end = lines.offset_to_position(source, range.end).unwrap();
        TextDocumentContentChangeEvent {
            range: Some(LspRange::new(
                LspPosition::new(start.line, start.character),
                LspPosition::new(end.line, end.character),
            )),
            range_length: None,
            text: text.to_owned(),
        }
    }

    /// Each ranged change must be interpreted against the text left by the previous one, so a
    /// batch applied incrementally has to equal the same edits applied directly.
    #[test]
    fn incremental_changes_match_direct_application() {
        const INSERTS: &[&str] = &["", "x", "hello", "\n", "a\nb", "  ", "é", "😀", "};\n"];

        for encoding in [PositionEncoding::Utf16, PositionEncoding::Utf8] {
            for seed in 1..400u64 {
                let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
                let mut text = String::from("fn main() {\n    let x = 1;\n    let y = 2;\n}\n");
                let mut expected = text.clone();
                let mut batch = Vec::new();

                for _ in 0..rng.below(5) + 1 {
                    // Pick a valid range over the *current* expected text.
                    let mut start = rng.below(expected.len() + 1);
                    while !expected.is_char_boundary(start) {
                        start -= 1;
                    }
                    let mut end = start + rng.below(expected.len() - start + 1);
                    while !expected.is_char_boundary(end) {
                        end -= 1;
                    }
                    let insert = INSERTS[rng.below(INSERTS.len())];
                    batch.push(change(&expected, start..end, insert, encoding));
                    expected.replace_range(start..end, insert);
                }

                let (actual, rejected) = apply_content_changes(&text, &batch, encoding);
                assert_eq!(rejected, 0, "seed {seed}, {encoding:?}: change rejected");
                assert_eq!(
                    actual, expected,
                    "seed {seed}, {encoding:?}: incremental application diverged"
                );
                text = actual;
                assert_eq!(text, expected);
            }
        }
    }

    #[test]
    fn a_full_replacement_discards_what_came_before() {
        let changes = vec![
            change("old\n", 0..3, "new", PositionEncoding::Utf16),
            TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: "replaced entirely\n".to_owned(),
            },
        ];
        let (text, rejected) = apply_content_changes("old\n", &changes, PositionEncoding::Utf16);
        assert_eq!(text, "replaced entirely\n");
        assert_eq!(rejected, 0);
    }

    /// An unresolvable range must be skipped and counted, never silently mis-applied.
    #[test]
    fn out_of_range_changes_are_rejected_not_guessed() {
        let source = "fn main() {}\n";
        let beyond = TextDocumentContentChangeEvent {
            range: Some(LspRange::new(
                LspPosition::new(99, 0),
                LspPosition::new(99, 5),
            )),
            range_length: None,
            text: "x".to_owned(),
        };
        let (text, rejected) = apply_content_changes(
            source,
            std::slice::from_ref(&beyond),
            PositionEncoding::Utf16,
        );
        assert_eq!(text, source, "buffer must be left untouched");
        assert_eq!(rejected, 1);

        // A surviving change in the same batch still applies.
        let good = change(source, 0..2, "FN", PositionEncoding::Utf16);
        let (text, rejected) =
            apply_content_changes(source, &[beyond, good], PositionEncoding::Utf16);
        assert_eq!(text, "FN main() {}\n");
        assert_eq!(rejected, 1);
    }

    #[test]
    fn positions_round_trip_through_both_encodings() {
        let source = "let e = 😀;\nlet f = é;\n";
        for encoding in [PositionEncoding::Utf16, PositionEncoding::Utf8] {
            let lines = LineIndex::new(source, encoding);
            for offset in 0..=source.len() {
                if !source.is_char_boundary(offset) {
                    continue;
                }
                let position = lines.offset_to_position(source, offset).unwrap();
                let lsp = LspPosition::new(position.line, position.character);
                assert_eq!(
                    position_offset(source, lsp, encoding),
                    Some(offset),
                    "{encoding:?} round trip failed at {offset}"
                );
            }
        }
    }
}
