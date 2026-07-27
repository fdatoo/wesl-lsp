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
    DidChangeWatchedFilesParams, DidChangeWatchedFilesRegistrationOptions,
    DidChangeWorkspaceFoldersParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    DidSaveTextDocumentParams, DocumentDiagnosticParams, DocumentDiagnosticReport,
    DocumentDiagnosticReportKind, DocumentDiagnosticReportResult, DocumentFormattingParams,
    DocumentHighlight, DocumentHighlightKind, DocumentHighlightParams,
    DocumentOnTypeFormattingOptions, DocumentOnTypeFormattingParams, DocumentRangeFormattingParams,
    DocumentSymbol, DocumentSymbolParams, DocumentSymbolResponse, Documentation, FileChangeType,
    FileOperationFilter, FileOperationPattern, FileOperationPatternKind,
    FileOperationRegistrationOptions, FileSystemWatcher, FoldingRange as LspFoldingRange,
    FoldingRangeKind, FoldingRangeParams, FoldingRangeProviderCapability,
    FullDocumentDiagnosticReport, GlobPattern, GotoDefinitionParams, GotoDefinitionResponse, Hover,
    HoverContents, HoverParams, HoverProviderCapability, InitializeParams, InitializeResult,
    InlayHint as LspInlayHint, InlayHintKind, InlayHintLabel, InlayHintOptions, InlayHintParams,
    InlayHintServerCapabilities, InsertTextFormat, Location as LspLocation, MarkupContent,
    MarkupKind, OneOf, ParameterInformation, ParameterLabel, Position as LspPosition,
    PositionEncodingKind, PrepareRenameResponse, PublishDiagnosticsParams, Range as LspRange,
    ReferenceParams, Registration, RegistrationParams, RelatedFullDocumentDiagnosticReport,
    RenameFilesParams, RenameOptions, RenameParams, SaveOptions, SelectionRange,
    SelectionRangeParams, SelectionRangeProviderCapability, ServerCapabilities, SignatureHelp,
    SignatureHelpOptions, SignatureHelpParams, SignatureInformation, SymbolInformation,
    SymbolKind as LspSymbolKind, TextDocumentContentChangeEvent, TextDocumentPositionParams,
    TextDocumentSyncCapability, TextDocumentSyncKind, TextDocumentSyncOptions,
    TextDocumentSyncSaveOptions, TextEdit, Url, WorkDoneProgressOptions, WorkspaceEdit,
    WorkspaceFileOperationsServerCapabilities, WorkspaceFoldersServerCapabilities,
    WorkspaceServerCapabilities, WorkspaceSymbolParams, WorkspaceSymbolResponse,
    notification::{
        DidChangeConfiguration, DidChangeTextDocument, DidChangeWatchedFiles,
        DidChangeWorkspaceFolders, DidCloseTextDocument, DidOpenTextDocument, DidSaveTextDocument,
        Notification as NotificationTrait, PublishDiagnostics,
    },
    request::{
        Completion, DocumentDiagnosticRequest, DocumentHighlightRequest, DocumentSymbolRequest,
        FoldingRangeRequest, Formatting, GotoDefinition, HoverRequest, InlayHintRequest,
        OnTypeFormatting, PrepareRenameRequest, RangeFormatting, References, RegisterCapability,
        Rename, Request as RequestTrait, SelectionRangeRequest, SignatureHelpRequest,
        WillRenameFiles, WorkspaceConfiguration, WorkspaceDiagnosticRefresh,
        WorkspaceSymbolRequest,
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

#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct DiagnosticSettings {
    enabled: bool,
    /// Opt in to `textDocument/diagnostic` instead of pushing.
    ///
    /// Off by default even for clients that advertise pull support. Zed advertises it (its
    /// `lsp_pull_diagnostics.enabled` defaults to true) but pull was tried against Zed and
    /// backed out — see the commit "Use push diagnostics for Zed compatibility", which removed
    /// exactly this capability. Honouring the advertisement alone would silently put every Zed
    /// user back on the path that did not work.
    pull: bool,
}

impl Default for DiagnosticSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            pull: false,
        }
    }
}

fn main() -> Result<()> {
    env_logger::init();
    let log_timing = std::env::args().any(|argument| argument == "--log-timing");
    let (connection, io_threads) = Connection::stdio();
    let (initialize_id, initialize_params_json) = connection.initialize_start()?;
    // lsp-types 0.95 maps this to `workspace.diagnostic` (singular) via
    // `DiagnosticWorkspaceClientCapabilities`, but the specification's actual JSON property is
    // `workspace.diagnostics` (plural — see "Diagnostics Refresh" in the LSP 3.17 spec, and the
    // frozen Zed fixture in the test suite, which sends the plural key). Reading the typed field
    // would silently miss every real client's advertisement, so this reads the raw value instead.
    let diagnostics_refresh_support = initialize_params_json
        .pointer("/capabilities/workspace/diagnostics/refreshSupport")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let initialize_params: InitializeParams = serde_json::from_value(initialize_params_json)?;
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
    // Both must agree: the client has to support pull *and* the workspace has to ask for it.
    // Advertising on client capability alone regresses Zed, which advertises pull by default.
    let pull_diagnostics = configuration.diagnostics.pull
        && initialize_params
            .capabilities
            .text_document
            .as_ref()
            .and_then(|text_document| text_document.diagnostic.as_ref())
            .is_some();
    // Whether the client can register file watchers dynamically — there is no static server
    // capability for `workspace/didChangeWatchedFiles`. The request itself is sent below,
    // after `initialize_finish`, once `initialized` is known to have arrived.
    let watch_files_dynamic_registration = initialize_params
        .capabilities
        .workspace
        .as_ref()
        .and_then(|workspace| workspace.did_change_watched_files.as_ref())
        .and_then(|watched_files| watched_files.dynamic_registration)
        .unwrap_or(false);
    let result = InitializeResult {
        capabilities: capabilities(pull_diagnostics, encoding),
        server_info: Some(lsp_types::ServerInfo {
            name: "wesl-lsp".into(),
            version: Some(env!("CARGO_PKG_VERSION").into()),
        }),
    };
    connection.initialize_finish(initialize_id, serde_json::to_value(result)?)?;
    // The client is only ready for server-to-client requests after `initialized`, which the
    // call above already waited for — sending this any earlier would race the handshake.
    if watch_files_dynamic_registration {
        let id = RequestId::from(WATCHED_FILES_REGISTRATION_ID.to_owned());
        connection
            .sender
            .send(Message::Request(watched_files_registration_request(id)))?;
    }
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
        document_uris: HashMap::new(),
        diagnostics_refresh_support,
        configuration,
        configuration_scope,
        supports_configuration,
        workspace_folders,
        encoding,
        pending_configuration: None,
        diagnostic_refresh_requests: 0,
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

const DIAGNOSTIC_REFRESH_REQUEST_ID: &str = "wesl-lsp/workspace-diagnostic-refresh";

/// Asks a pull client to re-issue `textDocument/diagnostic` for everything it has open. Pull
/// clients own the fetch, so this is the only way a settings change (e.g. diagnostics being
/// switched on) reaches documents already showing a stale — or, when disabled, missing — result.
fn workspace_diagnostic_refresh_request(id: RequestId) -> Request {
    Request::new(id, WorkspaceDiagnosticRefresh::METHOD.to_owned(), ())
}

const WATCHED_FILES_REGISTRATION_ID: &str = "wesl-lsp/watched-files-registration";

/// Registers interest in on-disk `.wesl`/`.wgsl` changes. Nothing else keeps `PackageIndex`
/// current once a root has been indexed — creating, editing or deleting a file outside the
/// editor would otherwise desync definitions, references and workspace symbols from disk
/// forever. There is no static server capability for this; dynamic registration via
/// `client/registerCapability` is the only mechanism the specification provides.
fn watched_files_registration_request(id: RequestId) -> Request {
    Request::new(
        id,
        RegisterCapability::METHOD.to_owned(),
        RegistrationParams {
            registrations: vec![Registration {
                id: WATCHED_FILES_REGISTRATION_ID.to_owned(),
                method: DidChangeWatchedFiles::METHOD.to_owned(),
                register_options: serde_json::to_value(DidChangeWatchedFilesRegistrationOptions {
                    watchers: vec![FileSystemWatcher {
                        glob_pattern: GlobPattern::String("**/*.{wesl,wgsl}".to_owned()),
                        kind: None,
                    }],
                })
                .ok(),
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
    /// The URI exactly as the client sent it in `didOpen`, keyed by the canonicalized path.
    /// `uri_path` normalises every incoming URI to a canonical path for internal bookkeeping,
    /// but a symlinked root means that path may not be the URI the client actually opened —
    /// re-emitting `Url::from_file_path` on the canonical path would then name a document the
    /// client never saw. This is the source of truth for URIs handed back to the client.
    document_uris: HashMap<PathBuf, Url>,
    /// Whether the client advertised `workspace.diagnostics.refreshSupport`; gates whether
    /// `apply_configuration` may ask a pull client to re-fetch after a settings change.
    diagnostics_refresh_support: bool,
    configuration: Configuration,
    configuration_scope: Option<Url>,
    supports_configuration: bool,
    /// Roots reported by the client, used only when no explicit root is configured.
    workspace_folders: Vec<PathBuf>,
    encoding: PositionEncoding,
    /// Set while a runtime settings refresh is in flight, so its response is recognised in
    /// the main loop rather than blocking for it.
    pending_configuration: Option<RequestId>,
    /// Monotonically increasing counter so each `workspace/diagnostic/refresh` request gets
    /// its own id; JSON-RPC requires ids to stay unique among concurrently outstanding
    /// requests, and nothing obliges a client to answer promptly. See
    /// [`Self::send_diagnostics_refresh`].
    diagnostic_refresh_requests: u64,
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
        let diagnostics_changed = configuration.diagnostics != self.configuration.diagnostics;
        self.configuration = configuration;
        if root_changed {
            // Discards every cached package, so the next request reindexes under the new root.
            self.analysis.set_roots(self.roots());
        }
        if !self.push_diagnostics {
            // Pull clients own the fetch; republishing does nothing for them, so the only way
            // to reach documents already showing a stale or missing result is to ask the
            // client to re-issue its pulls — but only when there is actually something new to
            // pull. `DiagnosticWorkspaceClientCapabilities` warns that a refresh "should be
            // used with absolute care", and `workspace/didChangeConfiguration` is broadcast
            // for any editor setting, not just this server's, so sending one unconditionally
            // would force a full re-pull of every open shader on unrelated settings churn.
            if diagnostics_changed || root_changed {
                self.send_diagnostics_refresh()?;
            }
            return Ok(());
        }
        // Diagnostics may have been switched on or off, and a new root changes what resolves.
        for path in self.versions.keys().cloned().collect::<Vec<_>>() {
            self.publish(&path)?;
        }
        Ok(())
    }

    /// Asks a pull client to re-issue `textDocument/diagnostic` for everything it has open; a
    /// no-op when the client never advertised `workspace.diagnostics.refreshSupport`. Used both
    /// after a settings change and after a watched-file event, the two ways a pull client's
    /// already-fetched diagnostics can go stale without it asking again.
    ///
    /// Each call mints a fresh id rather than reusing `DIAGNOSTIC_REFRESH_REQUEST_ID` verbatim:
    /// JSON-RPC requires ids to be unique among concurrently outstanding requests, and a client
    /// is under no obligation to answer promptly, so two of these can easily overlap. Nothing
    /// needs to track the id to recognise the eventual response — `run`'s `Message::Response`
    /// arm only ever matches `pending_configuration`, so a reply to this request already falls
    /// through unhandled, exactly as it did with the old constant id.
    fn send_diagnostics_refresh(&mut self) -> Result<()> {
        if !self.diagnostics_refresh_support {
            return Ok(());
        }
        let id = RequestId::from(format!(
            "{DIAGNOSTIC_REFRESH_REQUEST_ID}/{}",
            self.diagnostic_refresh_requests
        ));
        self.diagnostic_refresh_requests += 1;
        self.connection
            .sender
            .send(Message::Request(workspace_diagnostic_refresh_request(id)))?;
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
            DidChangeWatchedFiles::METHOD => {
                let params: DidChangeWatchedFilesParams =
                    serde_json::from_value(notification.params)?;
                for change in &params.changes {
                    let Ok(path) = uri_path(&change.uri) else {
                        continue;
                    };
                    match change.typ {
                        FileChangeType::DELETED => self.analysis.file_removed(&path),
                        _ => self.analysis.file_changed(&path),
                    }
                }
                if self.push_diagnostics {
                    // Same republish loop `DidChangeWorkspaceFolders` uses above.
                    for path in self.versions.keys().cloned().collect::<Vec<_>>() {
                        self.publish(&path)?;
                    }
                } else {
                    // Pull clients never see the republish above; a refresh is the only way an
                    // external change to an imported file reaches an already-open document.
                    self.send_diagnostics_refresh()?;
                }
            }
            DidOpenTextDocument::METHOD => {
                let params: DidOpenTextDocumentParams =
                    serde_json::from_value(notification.params)?;
                let path = uri_path(&params.text_document.uri)?;
                self.document_uris
                    .insert(path.clone(), params.text_document.uri.clone());
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
                        "ignored {rejected} malformed change(s) for {}; a position landing \
                         mid-character has no addressable offset, so the buffer may be stale \
                         until the next save",
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
                // Pull-only sessions never publish, so clearing here would violate the
                // one-mechanism-per-session invariant documented above `capabilities`.
                if self.push_diagnostics {
                    self.send_diagnostics(&path, Vec::new())?;
                }
                // Removed only after the clearing publish above, so a symlinked document's
                // last diagnostics notification still lands on the URI the client opened.
                self.document_uris.remove(&path);
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
                let offset = position_offset(
                    &source,
                    params.text_document_position.position,
                    self.encoding,
                );
                let items = offset.map(|offset| self.analysis.completions(&path, offset));
                let result = items.map(|items| {
                    CompletionResponse::Array(
                        items
                            .into_iter()
                            .filter_map(|item| self.completion_item(item))
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
                let location = offset.and_then(|offset| self.analysis.definition(&path, offset));
                let result: Option<GotoDefinitionResponse> = location
                    .and_then(|location| self.lsp_location_for(location))
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
                let locations = offset
                    .map(|offset| {
                        self.analysis
                            .references(&path, offset, params.context.include_declaration)
                    })
                    .unwrap_or_default();
                let result = locations
                    .into_iter()
                    .filter_map(|location| self.lsp_location_for(location))
                    .collect::<Vec<_>>();
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
                            let Some(range) = self.lsp_range_for(&edit.path, edit.range) else {
                                continue;
                            };
                            let Some(uri) = self.client_uri(&edit.path) else {
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
                        let Some(range) = self.lsp_range_for(&edit.path, edit.range) else {
                            continue;
                        };
                        let Some(uri) = self.client_uri(&edit.path) else {
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
                if !self.configuration.diagnostics.enabled {
                    // Mirrors `publish`'s push-mode behaviour: an empty report rather than an
                    // error, so a client toggling diagnostics off sees them clear instead of
                    // freeze.
                    let report = RelatedFullDocumentDiagnosticReport {
                        related_documents: None,
                        full_document_diagnostic_report: FullDocumentDiagnosticReport {
                            result_id: None,
                            items: Vec::new(),
                        },
                    };
                    return Ok(Response::new_ok(
                        request.id,
                        DocumentDiagnosticReportResult::Report(DocumentDiagnosticReport::Full(
                            report,
                        )),
                    ));
                }
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
                            self.client_uri(&other)?,
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
                let found = self.analysis.workspace_symbols(&params.query);
                let symbols = found
                    .into_iter()
                    .filter_map(|found| self.workspace_symbol_information(found))
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
                let found = self.analysis.document_symbols(&path);
                let symbols = found
                    .into_iter()
                    .filter_map(|symbol| self.document_symbol(&path, symbol))
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
            self.client_uri(path)
                .ok_or_else(|| anyhow::anyhow!("invalid file path"))?,
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
                                self.lsp_range_for(&related_path, related_range)?
                            };
                            Some(DiagnosticRelatedInformation {
                                location: LspLocation::new(self.client_uri(&related_path)?, range),
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

    /// The URI the client actually opened a document under, falling back to reconstructing
    /// one from the canonical path for documents that are not open (or never were). Using
    /// this instead of `Url::from_file_path` on the canonical path is what keeps every
    /// emitted URI resolvable by the client under a symlinked workspace root.
    fn client_uri(&self, path: &Path) -> Option<Url> {
        self.document_uris
            .get(path)
            .cloned()
            .or_else(|| Url::from_file_path(path).ok())
    }

    /// Converts a byte-offset range into an LSP range against the in-memory buffer when the
    /// document is open, falling back to disk otherwise — the same source resolution
    /// `lsp_diagnostics` uses, so an unsaved edit never desyncs the two.
    fn lsp_range_for(&self, path: &Path, range: std::ops::Range<usize>) -> Option<LspRange> {
        let source = self
            .analysis
            .source(path)
            .map(Cow::Borrowed)
            .or_else(|| std::fs::read_to_string(path).ok().map(Cow::Owned))?;
        let lines = LineIndex::new(&source, self.encoding);
        let start = lines.offset_to_position(&source, range.start)?;
        let end = lines.offset_to_position(&source, range.end)?;
        Some(LspRange::new(
            LspPosition::new(start.line, start.character),
            LspPosition::new(end.line, end.character),
        ))
    }

    fn lsp_location_for(&self, location: wesl_analysis::Location) -> Option<LspLocation> {
        Some(LspLocation::new(
            self.client_uri(&location.path)?,
            self.lsp_range_for(&location.path, location.range)?,
        ))
    }

    fn completion_item(&self, completion: AnalysisCompletion) -> Option<CompletionItem> {
        let additional_text_edits = if let Some(edit) = completion.additional_edit {
            Some(vec![TextEdit {
                range: self.lsp_range_for(&edit.path, edit.range)?,
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

    /// `SymbolInformation` is deprecated in favour of the 3.17 nested `WorkspaceSymbol`, but
    /// it is what every client understands, so the flat shape is the compatible choice here.
    #[allow(deprecated)]
    fn workspace_symbol_information(
        &self,
        found: AnalysisWorkspaceSymbol,
    ) -> Option<SymbolInformation> {
        let symbol = found.symbol;
        Some(SymbolInformation {
            name: symbol.name.to_string(),
            kind: lsp_symbol_kind(symbol.kind),
            tags: None,
            deprecated: None,
            location: LspLocation::new(
                self.client_uri(&symbol.path)?,
                self.lsp_range_for(&symbol.path, symbol.range)?,
            ),
            container_name: found.container,
        })
    }

    #[allow(deprecated)]
    fn document_symbol(&self, path: &Path, symbol: Symbol) -> Option<DocumentSymbol> {
        let range = self.lsp_range_for(path, symbol.full_range)?;
        let selection_range = self.lsp_range_for(path, symbol.range)?;
        let children = symbol
            .children
            .into_iter()
            .filter_map(|child| self.document_symbol(path, child))
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
}

/// Applies content changes in order, returning the new text and how many were rejected.
///
/// A ranged change is incremental, so both endpoints resolve against the text as it stands
/// immediately before that change — not against the original. Getting this wrong desynchronises
/// the server's buffer from the editor's silently: no error is raised, every later answer is
/// computed against the wrong text, and only a save resynchronises. Out-of-range endpoints
/// clamp to the document per the LSP `Position` spec instead of being rejected; a change is
/// counted as rejected only when an endpoint is genuinely unaddressable — a column landing
/// mid-character or mid-surrogate-pair, which has no offset to clamp to in the first place.
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

/// File-operation params carry URIs as plain strings rather than parsed `Url`s.
fn file_uri_path(uri: &str) -> Option<PathBuf> {
    Url::parse(uri).ok()?.to_file_path().ok()
}

fn uri_path(uri: &Url) -> Result<PathBuf> {
    let path = uri
        .to_file_path()
        .map_err(|_| anyhow::anyhow!("URI is not a file: {uri}"))?;
    Ok(canonicalize_best_effort(path))
}

/// Canonicalizes `path`, tolerating the file itself already being gone.
///
/// `canonicalize` calls `realpath(3)`, which fails with ENOENT the instant the leaf no longer
/// exists — exactly the case for a `FileChangeType::DELETED` watched-file event, or a
/// `close()` racing an external delete. Falling back to the raw, non-canonical path there (as
/// opposed to here) would desync the two: a `created`/`changed` event for the very same URI
/// canonicalizes fine, since the file still exists then, so under a symlinked root a delete
/// would end up filed under a different path than the create was, and callers that match by a
/// canonical root prefix (like `remove_from_cached_packages`) would silently drop the removal.
/// Canonicalizing the parent instead keeps the deleted path on the same name everything else
/// uses; the raw path is a last resort for when the parent is gone too (e.g. the whole
/// directory was removed).
fn canonicalize_best_effort(path: PathBuf) -> PathBuf {
    if let Ok(canonical) = path.canonicalize() {
        return canonical;
    }
    match (path.parent(), path.file_name()) {
        (Some(parent), Some(file_name)) => parent
            .canonicalize()
            .map(|canonical_parent| canonical_parent.join(file_name))
            .unwrap_or(path),
        _ => path,
    }
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

    /// Out-of-range positions clamp per the LSP `Position` spec ("character greater than the
    /// line length defaults back to the line length"), so a spec-legal oversized range applies
    /// instead of silently desynchronising the buffer. Only a genuinely malformed position —
    /// one landing mid-character — is skipped and counted.
    #[test]
    fn out_of_range_changes_clamp_and_malformed_ones_are_rejected() {
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
        assert_eq!(
            text, "fn main() {}\nx",
            "line past EOF clamps to document end"
        );
        assert_eq!(rejected, 0);

        // A character past the end of a line that *does* exist clamps to that line's content
        // rather than past its newline — a distinct code path from the line-past-EOF case
        // above, which takes `position_to_offset`'s other early return. Getting this wrong
        // would silently swallow the newline and merge two lines.
        let past_line_end = TextDocumentContentChangeEvent {
            range: Some(LspRange::new(
                LspPosition::new(0, 5),
                LspPosition::new(0, 99),
            )),
            range_length: None,
            text: "X".to_owned(),
        };
        let (text, rejected) = apply_content_changes(
            source,
            std::slice::from_ref(&past_line_end),
            PositionEncoding::Utf16,
        );
        assert_eq!(
            text, "fn maX\n",
            "character past a valid line's end clamps to the line, not into the newline"
        );
        assert_eq!(rejected, 0);

        // A UTF-8 column landing inside a multi-byte character is unaddressable: skipped and
        // counted, with the buffer left untouched, and a later change in the batch still applies.
        let emoji = "😀ok\n";
        let malformed = TextDocumentContentChangeEvent {
            range: Some(LspRange::new(
                LspPosition::new(0, 1),
                LspPosition::new(0, 2),
            )),
            range_length: None,
            text: "x".to_owned(),
        };
        let good = change(emoji, 4..6, "OK", PositionEncoding::Utf8);
        let (text, rejected) =
            apply_content_changes(emoji, &[malformed, good], PositionEncoding::Utf8);
        assert_eq!(text, "😀OK\n");
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
