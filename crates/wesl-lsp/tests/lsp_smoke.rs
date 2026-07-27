use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    path::PathBuf,
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc,
    thread,
    time::Duration,
};

use serde_json::{Value, json};

use tempfile::tempdir;

/// Long enough for a cold package index on a large workspace, short enough that a wedged
/// server fails the run instead of hanging the whole suite.
const REPLY_TIMEOUT: Duration = Duration::from_secs(30);

/// Distinguishes a clean end of stream (`Eof`, which the reader thread folds into simply
/// exiting, so `Client::receive` sees `RecvTimeoutError::Disconnected`) from a malformed
/// message (`Protocol`), which the thread instead posts to the channel as a sentinel before
/// exiting — see `Client::start`. Without this distinction a crashing server's truncated
/// output looked identical to a clean shutdown, and `Client::receive` reported a misleading
/// "no message within 30s" instead of the real cause.
#[derive(Debug)]
enum ReadError {
    Eof,
    Protocol(String),
}

fn read_message(reader: &mut impl BufRead) -> Result<Value, ReadError> {
    let mut content_length = None;
    loop {
        let mut line = String::new();
        let bytes = reader
            .read_line(&mut line)
            .map_err(|error| ReadError::Protocol(format!("failed to read header: {error}")))?;
        if bytes == 0 {
            return Err(ReadError::Eof);
        }
        if line == "\r\n" {
            break;
        }
        if let Some(value) = line.strip_prefix("Content-Length: ") {
            content_length = Some(value.trim().parse::<usize>().map_err(|error| {
                ReadError::Protocol(format!("malformed Content-Length {value:?}: {error}"))
            })?);
        }
    }
    let content_length = content_length
        .ok_or_else(|| ReadError::Protocol("header block had no Content-Length".to_owned()))?;
    let mut body = vec![0; content_length];
    Read::read_exact(reader, &mut body).map_err(|error| {
        ReadError::Protocol(format!(
            "short body (wanted {content_length} bytes): {error}"
        ))
    })?;
    serde_json::from_slice(&body)
        .map_err(|error| ReadError::Protocol(format!("non-JSON body: {error}")))
}

#[test]
fn read_message_distinguishes_eof_from_protocol_errors() {
    use std::io::Cursor;

    // Nothing was ever written: a clean end of stream, not a protocol error.
    assert!(matches!(
        read_message(&mut Cursor::new(b"".as_slice())),
        Err(ReadError::Eof)
    ));

    // A header block whose Content-Length value doesn't parse.
    assert!(matches!(
        read_message(&mut Cursor::new(
            b"Content-Length: not-a-number\r\n\r\n".as_slice()
        )),
        Err(ReadError::Protocol(_))
    ));

    // Content-Length overstates what actually follows.
    assert!(matches!(
        read_message(&mut Cursor::new(b"Content-Length: 10\r\n\r\n{}".as_slice())),
        Err(ReadError::Protocol(_))
    ));

    // A body of exactly the promised length that still isn't valid JSON.
    assert!(matches!(
        read_message(&mut Cursor::new(b"Content-Length: 3\r\n\r\nabc".as_slice())),
        Err(ReadError::Protocol(_))
    ));

    // A well-formed message still parses despite the stricter error type.
    let body = br#"{"jsonrpc":"2.0","method":"ping"}"#;
    let mut framed = format!("Content-Length: {}\r\n\r\n", body.len()).into_bytes();
    framed.extend_from_slice(body);
    assert_eq!(
        read_message(&mut Cursor::new(framed.as_slice())).unwrap(),
        json!({"jsonrpc": "2.0", "method": "ping"})
    );
}

struct Client {
    child: Child,
    input: Option<ChildStdin>,
    incoming: mpsc::Receiver<Result<Value, String>>,
}

impl Client {
    fn start() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_wesl-lsp"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let input = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        // Reading on a thread keeps a wedged server from hanging the whole run.
        let (sender, incoming) = mpsc::channel();
        thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            loop {
                match read_message(&mut reader) {
                    Ok(message) => {
                        if sender.send(Ok(message)).is_err() {
                            break;
                        }
                    }
                    Err(ReadError::Eof) => break,
                    Err(ReadError::Protocol(error)) => {
                        let _ = sender.send(Err(error));
                        break;
                    }
                }
            }
        });
        Self {
            child,
            input: Some(input),
            incoming,
        }
    }

    fn send(&mut self, value: Value) {
        let body = serde_json::to_vec(&value).unwrap();
        write!(
            self.input.as_mut().unwrap(),
            "Content-Length: {}\r\n\r\n",
            body.len()
        )
        .unwrap();
        self.input.as_mut().unwrap().write_all(&body).unwrap();
        self.input.as_mut().unwrap().flush().unwrap();
    }

    fn receive(&mut self) -> Value {
        match self.incoming.recv_timeout(REPLY_TIMEOUT) {
            Ok(Ok(message)) => message,
            Ok(Err(protocol_error)) => {
                panic!("server produced a malformed message: {protocol_error}")
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => panic!("server closed stdout"),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                panic!("server produced no message within {REPLY_TIMEOUT:?}")
            }
        }
    }

    fn receive_response(&mut self, id: i64) -> Value {
        loop {
            let message = self.receive();
            if message["id"] == id {
                return message;
            }
        }
    }

    fn receive_diagnostics(&mut self, uri: &lsp_types::Url) -> Value {
        let expected_path = uri.to_file_path().unwrap().canonicalize().unwrap();
        loop {
            let message = self.receive();
            if message["method"].as_str() == Some("textDocument/publishDiagnostics")
                && message["params"]["uri"]
                    .as_str()
                    .and_then(|uri| lsp_types::Url::parse(uri).ok())
                    .and_then(|uri| uri.to_file_path().ok())
                    .and_then(|path| path.canonicalize().ok())
                    .is_some_and(|path| path == expected_path)
            {
                return message;
            }
        }
    }

    fn shutdown(&mut self) {
        eprintln!("sending shutdown");
        self.send(json!({"jsonrpc": "2.0", "id": 99, "method": "shutdown"}));
        let response = self.receive_response(99);
        assert_eq!(response["id"], 99);
        self.send(json!({"jsonrpc": "2.0", "method": "exit"}));
        self.input.take();

        eprintln!("waiting for server exit");
        assert!(self.child.wait().unwrap().success());
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

/// With `WESL_LSP_REQUIRE_CORPORA` set to a non-empty value other than `"0"`, a missing corpus
/// fails instead of skipping.
///
/// Every corpus gate returns early when its input is absent, which means a half-successful
/// `fetch-corpus` turns the whole suite green while proving nothing. CI sets this so that
/// silence becomes a failure. `var_os(..).is_some()` would treat an *empty* override as "set",
/// but CI's skip-mode step exports `WESL_LSP_REQUIRE_CORPORA: ""` expecting that to mean off
/// (GitHub Actions exports empty-valued env vars rather than unsetting them), so empty and
/// `"0"` both count as off here.
fn require_corpora() -> bool {
    std::env::var("WESL_LSP_REQUIRE_CORPORA").is_ok_and(|value| !value.is_empty() && value != "0")
}

/// Reads the Seclorum shader corpus root from `WESL_LSP_SECLORUM_SHADERS` if set, so the one
/// end-to-end pass over definition/references/rename/documentSymbol/hover/completion is not
/// wired to one machine's checkout layout. Falls back to the relative path used on the
/// machine this was written on; the caller's `is_dir()` check skips the test either way when
/// neither exists. Canonicalized because the URL parser strips `..` segments on the wire:
/// a dotted path would make `receive_diagnostics`'s verbatim URI comparison unsatisfiable.
fn shaders() -> PathBuf {
    let root = std::env::var_os("WESL_LSP_SECLORUM_SHADERS")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../../Seclorum/novus/crates/novus-render/shaders")
        });
    root.canonicalize().unwrap_or(root)
}

fn position(source: &str, offset: usize) -> Value {
    let index = wesl_analysis::LineIndex::new(source, wesl_analysis::PositionEncoding::default());
    let position = index.offset_to_position(source, offset).unwrap();
    json!({"line": position.line, "character": position.character})
}

/// Requests inlay hints over the whole document and returns just the labels.
fn inlay_labels(client: &mut Client, id: i64, uri: &lsp_types::Url, source: &str) -> Vec<String> {
    client.send(json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "textDocument/inlayHint",
        "params": {
            "textDocument": {"uri": uri},
            "range": {
                "start": position(source, 0),
                "end": position(source, source.len())
            }
        }
    }));
    client.receive_response(id)["result"]
        .as_array()
        .unwrap()
        .iter()
        .map(|hint| hint["label"].as_str().unwrap_or_default().to_owned())
        .collect()
}

#[test]
fn requests_workspace_configuration_when_initialization_root_is_absent() {
    let temp = tempdir().unwrap();
    let workspace = temp.path();
    let shaders = workspace.join("shaders");
    fs::create_dir(&shaders).unwrap();
    fs::write(shaders.join("wesl.toml"), "").unwrap();
    fs::write(workspace.join("shared.wesl"), "const value: f32 = 1.0;\n").unwrap();
    let path = shaders.join("main.wesl");
    let source = "import package::shared::value;\nfn main() { let x: f32 = value; }\n";
    fs::write(&path, source).unwrap();
    let workspace_uri = lsp_types::Url::from_file_path(workspace).unwrap();
    let uri = lsp_types::Url::from_file_path(&path).unwrap();

    let mut client = Client::start();
    client.send(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "capabilities": {"workspace": {"configuration": true}},
            "workspaceFolders": [{"uri": workspace_uri, "name": "workspace"}]
        }
    }));
    let initialized = client.receive_response(1);
    assert_eq!(initialized["id"], 1);
    assert!(
        initialized["result"]["capabilities"]
            .get("diagnosticProvider")
            .is_none(),
        "push-only server must not advertise pull diagnostics"
    );
    assert_eq!(
        initialized["result"]["capabilities"]["completionProvider"]["triggerCharacters"],
        json!(["."])
    );
    client.send(json!({"jsonrpc": "2.0", "method": "initialized", "params": {}}));
    let configuration = client.receive();
    assert_eq!(configuration["method"], "workspace/configuration");
    assert_eq!(configuration["params"]["items"][0]["section"], "wesl-lsp");
    client.send(json!({
        "jsonrpc": "2.0",
        "id": configuration["id"].clone(),
        "result": [{"root": "."}]
    }));
    client.send(json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didOpen",
        "params": {
            "textDocument": {
                "uri": uri,
                "languageId": "wesl",
                "version": 1,
                "text": source
            }
        }
    }));

    let diagnostics = client.receive_diagnostics(&uri);
    assert_eq!(diagnostics["params"]["diagnostics"], json!([]));
    client.shutdown();
}

#[test]
fn struct_members_complete_reference_and_clear_diagnostics() {
    let temp = tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let path = root.join("main.wesl");
    let source = "struct Output { value: f32, }\nfn main() { var out: Output; out.value = 1.0; }\n";
    fs::write(&path, source).unwrap();
    let uri = lsp_types::Url::from_file_path(&path).unwrap();
    let mut client = Client::start();

    client.send(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "capabilities": {},
            "initializationOptions": {"root": root},
            "rootUri": null
        }
    }));
    assert_eq!(client.receive_response(1)["id"], 1);
    client.send(json!({"jsonrpc": "2.0", "method": "initialized", "params": {}}));
    client.send(json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didOpen",
        "params": {
            "textDocument": {
                "uri": uri,
                "languageId": "wesl",
                "version": 1,
                "text": source
            }
        }
    }));
    assert_eq!(
        client.receive_diagnostics(&uri)["params"]["diagnostics"],
        json!([])
    );

    let usage = source.rfind("value").unwrap();
    client.send(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "textDocument/completion",
        "params": {
            "textDocument": {"uri": uri},
            "position": position(source, usage)
        }
    }));
    let completions = client.receive_response(2);
    assert!(
        completions["result"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["label"] == "value"),
        "{completions:#?}"
    );

    let declaration = source.find("value:").unwrap();
    client.send(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "textDocument/references",
        "params": {
            "textDocument": {"uri": uri},
            "position": position(source, declaration),
            "context": {"includeDeclaration": true}
        }
    }));
    assert_eq!(
        client.receive_response(3)["result"]
            .as_array()
            .unwrap()
            .len(),
        2
    );

    let broken = source.replace("out.value", "out.missing");
    client.send(json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didChange",
        "params": {
            "textDocument": {"uri": uri, "version": 2},
            "contentChanges": [{"text": broken}]
        }
    }));
    let diagnostics = client.receive_diagnostics(&uri);
    let diagnostics = diagnostics["params"]["diagnostics"].as_array().unwrap();
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0]["message"], "type has no member missing");
    assert_eq!(
        diagnostics[0]["range"]["start"],
        position(&broken, broken.find("missing").unwrap())
    );

    client.send(json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didChange",
        "params": {
            "textDocument": {"uri": uri, "version": 3},
            "contentChanges": [{"text": source}]
        }
    }));
    assert_eq!(
        client.receive_diagnostics(&uri)["params"]["diagnostics"],
        json!([])
    );
    client.shutdown();
}

#[test]
fn publishes_clean_then_isolated_import_diagnostics() {
    let shader_root = shaders();
    if !shader_root.is_dir() {
        assert!(
            !require_corpora(),
            "Seclorum shader corpus missing at {}; set WESL_LSP_SECLORUM_SHADERS to the \
             checkout (WESL_LSP_REQUIRE_CORPORA unset or \"0\" skips this check)",
            shader_root.display()
        );
        eprintln!("skipping private Seclorum LSP smoke test");
        return;
    }
    let path = shader_root.join("sky.wesl");
    let source = fs::read_to_string(&path).unwrap();
    let uri = lsp_types::Url::from_file_path(&path).unwrap();
    let mut client = Client::start();

    client.send(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "capabilities": {},
            "initializationOptions": {"root": shader_root},
            "rootUri": null
        }
    }));
    eprintln!("waiting for initialize");
    let initialized = client.receive_response(1);
    assert_eq!(initialized["id"], 1);
    client.send(json!({"jsonrpc": "2.0", "method": "initialized", "params": {}}));
    client.send(json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didOpen",
        "params": {
            "textDocument": {
                "uri": uri,
                "languageId": "wesl",
                "version": 1,
                "text": source
            }
        }
    }));
    eprintln!("waiting for clean diagnostics");
    let clean = client.receive_diagnostics(&uri);
    assert_eq!(clean["method"], "textDocument/publishDiagnostics");
    assert_eq!(clean["params"]["diagnostics"], json!([]));

    let broken = source.replacen("package::sky_common", "package::sky_commonX", 1);
    client.send(json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didChange",
        "params": {
            "textDocument": {"uri": uri, "version": 2},
            "contentChanges": [{"text": broken}]
        }
    }));
    eprintln!("waiting for changed diagnostics");
    let diagnostics = client.receive_diagnostics(&uri);
    eprintln!("received changed diagnostics");
    assert_eq!(diagnostics["method"], "textDocument/publishDiagnostics");
    let diagnostics = diagnostics["params"]["diagnostics"].as_array().unwrap();
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(diagnostics[0]["range"]["start"]["line"], 0);
    assert!(
        diagnostics[0]["message"]
            .as_str()
            .unwrap()
            .contains("sky_commonX")
    );

    let clouds_path = shader_root.join("clouds_map.wesl");
    let clouds_source = fs::read_to_string(&clouds_path).unwrap();
    let clouds_uri = lsp_types::Url::from_file_path(&clouds_path).unwrap();
    client.send(json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didOpen",
        "params": {
            "textDocument": {
                "uri": clouds_uri,
                "languageId": "wesl",
                "version": 1,
                "text": clouds_source
            }
        }
    }));
    assert_eq!(
        client.receive_diagnostics(&clouds_uri)["params"]["diagnostics"],
        json!([]),
        "clouds_map.wesl should be clean"
    );
    let call = clouds_source.find("wx_coverage(").unwrap();
    let call_position = position(&clouds_source, call + 2);

    client.send(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "textDocument/definition",
        "params": {
            "textDocument": {"uri": clouds_uri},
            "position": call_position
        }
    }));
    let definition = client.receive_response(2);
    assert_eq!(definition["id"], 2);
    assert!(
        definition["result"]["uri"]
            .as_str()
            .unwrap()
            .ends_with("weather_common.wesl")
    );
    assert_eq!(definition["result"]["range"]["start"]["line"], 54);

    client.send(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "textDocument/references",
        "params": {
            "textDocument": {"uri": clouds_uri},
            "position": call_position,
            "context": {"includeDeclaration": true}
        }
    }));
    let references = client.receive_response(3);
    assert_eq!(references["id"], 3);
    assert!(references["result"].as_array().unwrap().len() >= 4);

    client.send(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "textDocument/rename",
        "params": {
            "textDocument": {"uri": clouds_uri},
            "position": call_position,
            "newName": "wx_coverage_renamed"
        }
    }));
    let rename = client.receive_response(4);
    assert_eq!(rename["id"], 4);
    assert!(rename["result"]["changes"].as_object().unwrap().len() >= 4);

    client.send(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "textDocument/documentSymbol",
        "params": {"textDocument": {"uri": clouds_uri}}
    }));
    let symbols = client.receive_response(5);
    assert_eq!(symbols["id"], 5);
    assert!(!symbols["result"].as_array().unwrap().is_empty());

    client.send(json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "textDocument/hover",
        "params": {
            "textDocument": {"uri": clouds_uri},
            "position": call_position
        }
    }));
    let hover = client.receive_response(6);
    assert_eq!(hover["id"], 6);
    assert!(
        hover["result"]["contents"]["value"]
            .as_str()
            .unwrap()
            .contains("fn wx_coverage")
    );

    client.send(json!({
        "jsonrpc": "2.0",
        "id": 7,
        "method": "textDocument/completion",
        "params": {
            "textDocument": {"uri": clouds_uri},
            "position": call_position
        }
    }));
    let completion = client.receive_response(7);
    assert_eq!(completion["id"], 7);
    let items = completion["result"].as_array().unwrap();
    assert!(items.iter().any(|item| item["label"] == "wx_coverage"));
    assert!(items.iter().any(|item| item["label"] == "sin"));
    assert!(
        items
            .iter()
            .any(|item| item.get("additionalTextEdits").is_some())
    );

    let terrain_path = shader_root.join("terrain.wesl");
    let terrain_source = fs::read_to_string(&terrain_path).unwrap();
    let terrain_uri = lsp_types::Url::from_file_path(&terrain_path).unwrap();
    let terrain_with_error =
        format!("{terrain_source}\nfn wesl_lsp_type_test() {{ let x: f32 = vec3(1.0); }}\n");
    client.send(json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didOpen",
        "params": {
            "textDocument": {
                "uri": terrain_uri,
                "languageId": "wesl",
                "version": 1,
                "text": terrain_with_error
            }
        }
    }));
    let type_diagnostics = client.receive_diagnostics(&terrain_uri);
    let type_diagnostics = type_diagnostics["params"]["diagnostics"]
        .as_array()
        .unwrap();
    assert_eq!(type_diagnostics.len(), 1, "{type_diagnostics:#?}");
    assert_eq!(
        type_diagnostics[0]["message"],
        "type mismatch: expected f32, found vec3<f32>"
    );
    assert_eq!(
        type_diagnostics[0]["relatedInformation"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    let blit_path = shader_root.join("blit.wesl");
    let blit_source = fs::read_to_string(&blit_path).unwrap();
    let blit_uri = lsp_types::Url::from_file_path(&blit_path).unwrap();
    client.send(json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didOpen",
        "params": {
            "textDocument": {
                "uri": blit_uri,
                "languageId": "wesl",
                "version": 1,
                "text": blit_source
            }
        }
    }));
    let blit_diagnostics = client.receive_diagnostics(&blit_uri);
    assert!(
        blit_diagnostics["params"]["diagnostics"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    client.send(json!({
        "jsonrpc": "2.0",
        "id": 8,
        "method": "textDocument/formatting",
        "params": {
            "textDocument": {"uri": blit_uri},
            "options": {"tabSize": 2, "insertSpaces": true}
        }
    }));
    let formatted = client.receive_response(8);
    assert_eq!(formatted["id"], 8);
    let formatted = formatted["result"][0]["newText"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(formatted.contains("\n  @builtin(position)\n  clip_position"));
    assert!(formatted.contains("// Present blit for the windowed path."));
    client.send(json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didChange",
        "params": {
            "textDocument": {"uri": blit_uri, "version": 2},
            "contentChanges": [{"text": formatted}]
        }
    }));
    client.send(json!({
        "jsonrpc": "2.0",
        "id": 9,
        "method": "textDocument/formatting",
        "params": {
            "textDocument": {"uri": blit_uri},
            "options": {"tabSize": 2, "insertSpaces": true}
        }
    }));
    let idempotent = client.receive_response(9);
    assert_eq!(idempotent["id"], 9);
    assert!(idempotent["result"].is_null());

    eprintln!("diagnostics verified");
    client.shutdown();
}

#[test]
fn highlights_and_prepares_rename() {
    let temp = tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let path = root.join("main.wesl");
    let source = "fn scale(factor: f32) -> f32 { return factor * factor; }\nfn main() { let x = scale(2.0); }\n";
    fs::write(&path, source).unwrap();
    let uri = lsp_types::Url::from_file_path(&path).unwrap();
    let mut client = Client::start();

    client.send(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "capabilities": {},
            "initializationOptions": {"root": root},
            "rootUri": null
        }
    }));
    let initialized = client.receive_response(1);
    assert_eq!(
        initialized["result"]["capabilities"]["documentHighlightProvider"],
        json!(true)
    );
    assert_eq!(
        initialized["result"]["capabilities"]["renameProvider"]["prepareProvider"],
        json!(true)
    );
    client.send(json!({"jsonrpc": "2.0", "method": "initialized", "params": {}}));
    client.send(json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didOpen",
        "params": {
            "textDocument": {"uri": uri, "languageId": "wesl", "version": 1, "text": source}
        }
    }));
    assert_eq!(
        client.receive_diagnostics(&uri)["params"]["diagnostics"],
        json!([])
    );

    // `factor` is declared once and used twice, all within this file.
    let parameter = source.find("factor").unwrap();
    client.send(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "textDocument/documentHighlight",
        "params": {
            "textDocument": {"uri": uri},
            "position": position(source, parameter)
        }
    }));
    let highlights = client.receive_response(2);
    assert_eq!(
        highlights["result"].as_array().unwrap().len(),
        3,
        "{highlights:#?}"
    );

    client.send(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "textDocument/prepareRename",
        "params": {
            "textDocument": {"uri": uri},
            "position": position(source, parameter)
        }
    }));
    let prepared = client.receive_response(3);
    assert_eq!(
        prepared["result"]["start"],
        position(source, parameter),
        "{prepared:#?}"
    );
    assert_eq!(
        prepared["result"]["end"],
        position(source, parameter + "factor".len())
    );

    let call_site = source.find("scale(2.0)").unwrap();
    client.send(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "textDocument/prepareRename",
        "params": {
            "textDocument": {"uri": uri},
            "position": position(source, call_site)
        }
    }));
    let user_symbol = client.receive_response(4);
    assert!(
        user_symbol["result"].is_object(),
        "user functions stay renameable: {user_symbol:#?}"
    );

    client.shutdown();
}

#[test]
fn workspace_symbols_reach_unopened_files() {
    let temp = tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    // Only main.wesl is opened; types.wesl must still be searchable.
    fs::write(
        root.join("types.wesl"),
        "struct Camera { projection: mat4x4<f32>, }\n",
    )
    .unwrap();
    let path = root.join("main.wesl");
    let source = "fn project() -> f32 { return 1.0; }\n";
    fs::write(&path, source).unwrap();
    let uri = lsp_types::Url::from_file_path(&path).unwrap();
    let mut client = Client::start();

    client.send(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "capabilities": {},
            "initializationOptions": {"root": root},
            "rootUri": null
        }
    }));
    let initialized = client.receive_response(1);
    assert_eq!(
        initialized["result"]["capabilities"]["workspaceSymbolProvider"],
        json!(true)
    );
    client.send(json!({"jsonrpc": "2.0", "method": "initialized", "params": {}}));
    client.send(json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didOpen",
        "params": {
            "textDocument": {"uri": uri, "languageId": "wesl", "version": 1, "text": source}
        }
    }));
    client.receive_diagnostics(&uri);

    client.send(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "workspace/symbol",
        "params": {"query": "proj"}
    }));
    let symbols = client.receive_response(2);
    let found = symbols["result"].as_array().unwrap();
    let projection = found
        .iter()
        .find(|symbol| symbol["name"] == "projection")
        .unwrap_or_else(|| panic!("{symbols:#?}"));
    assert_eq!(projection["containerName"], "Camera");
    assert!(
        projection["location"]["uri"]
            .as_str()
            .unwrap()
            .ends_with("types.wesl"),
        "{projection:#?}"
    );
    assert!(
        found.iter().any(|symbol| symbol["name"] == "project"),
        "{symbols:#?}"
    );

    client.shutdown();
}

#[test]
fn folding_ranges_survive_a_broken_buffer() {
    let temp = tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let path = root.join("main.wesl");
    let source =
        "// a comment\n// spanning lines\nfn main() {\n    let x = 1;\n    let y = 2;\n}\n";
    fs::write(&path, source).unwrap();
    let uri = lsp_types::Url::from_file_path(&path).unwrap();
    let mut client = Client::start();

    client.send(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "capabilities": {},
            "initializationOptions": {"root": root},
            "rootUri": null
        }
    }));
    let initialized = client.receive_response(1);
    assert_eq!(
        initialized["result"]["capabilities"]["foldingRangeProvider"],
        json!(true)
    );
    client.send(json!({"jsonrpc": "2.0", "method": "initialized", "params": {}}));
    client.send(json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didOpen",
        "params": {
            "textDocument": {"uri": uri, "languageId": "wesl", "version": 1, "text": source}
        }
    }));
    client.receive_diagnostics(&uri);

    client.send(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "textDocument/foldingRange",
        "params": {"textDocument": {"uri": uri}}
    }));
    let folds = client.receive_response(2);
    let ranges = folds["result"].as_array().unwrap();
    assert!(
        ranges.iter().any(|range| range["kind"] == "comment"
            && range["startLine"] == 0
            && range["endLine"] == 1),
        "{folds:#?}"
    );
    // The body opens on line 2 and closes on line 5; the `}` stays visible.
    assert!(
        ranges.iter().any(|range| range["kind"] == "region"
            && range["startLine"] == 2
            && range["endLine"] == 4),
        "{folds:#?}"
    );

    // Folding is token-based, so it must keep working once the buffer stops parsing.
    let broken = source.replace("fn main() {", "fn main( {");
    client.send(json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didChange",
        "params": {
            "textDocument": {"uri": uri, "version": 2},
            "contentChanges": [{"text": broken}]
        }
    }));
    client.send(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "textDocument/foldingRange",
        "params": {"textDocument": {"uri": uri}}
    }));
    let broken_folds = client.receive_response(3);
    assert!(
        broken_folds["result"]
            .as_array()
            .unwrap()
            .iter()
            .any(|range| range["kind"] == "region"),
        "{broken_folds:#?}"
    );

    client.shutdown();
}

#[test]
fn selection_ranges_nest_outward() {
    let temp = tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let path = root.join("main.wesl");
    let source = "fn main() {\n    let value = clamp(alpha, 0.0, 1.0);\n}\n";
    fs::write(&path, source).unwrap();
    let uri = lsp_types::Url::from_file_path(&path).unwrap();
    let mut client = Client::start();

    client.send(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "capabilities": {},
            "initializationOptions": {"root": root},
            "rootUri": null
        }
    }));
    let initialized = client.receive_response(1);
    assert_eq!(
        initialized["result"]["capabilities"]["selectionRangeProvider"],
        json!(true)
    );
    client.send(json!({"jsonrpc": "2.0", "method": "initialized", "params": {}}));
    client.send(json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didOpen",
        "params": {
            "textDocument": {"uri": uri, "languageId": "wesl", "version": 1, "text": source}
        }
    }));
    client.receive_diagnostics(&uri);

    let alpha = source.find("alpha").unwrap();
    client.send(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "textDocument/selectionRange",
        "params": {
            "textDocument": {"uri": uri},
            "positions": [position(source, alpha)]
        }
    }));
    let selections = client.receive_response(2);
    let innermost = &selections["result"][0];
    assert_eq!(innermost["range"]["start"], position(source, alpha));
    assert_eq!(
        innermost["range"]["end"],
        position(source, alpha + "alpha".len())
    );

    // Each parent must strictly contain its child, ending at the whole document.
    let mut depth = 1;
    let mut node = innermost;
    while node.get("parent").is_some_and(|parent| !parent.is_null()) {
        let parent = &node["parent"];
        let child_start = &node["range"]["start"];
        let parent_start = &parent["range"]["start"];
        assert!(
            parent_start["line"].as_u64() < child_start["line"].as_u64()
                || (parent_start["line"] == child_start["line"]
                    && parent_start["character"].as_u64() <= child_start["character"].as_u64()),
            "parent must start at or before child: {selections:#?}"
        );
        node = parent;
        depth += 1;
    }
    assert!(depth >= 4, "expected a multi-level chain: {selections:#?}");
    assert_eq!(node["range"]["start"], json!({"line": 0, "character": 0}));

    client.shutdown();
}

#[test]
fn signature_help_covers_builtins_user_functions_and_constructors() {
    let temp = tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let path = root.join("main.wesl");
    let source = concat!(
        "struct Camera { origin: vec3<f32>, focal: f32, }\n",
        "fn shade(albedo: vec3<f32>, roughness: f32) -> f32 { return roughness; }\n",
        "fn main() {\n",
        "    let a = clamp(1.0, 0.0, 1.0);\n",
        "    let b = shade(vec3(1.0), 0.5);\n",
        "    let c = Camera(vec3(0.0), 1.0);\n",
        "}\n",
    );
    fs::write(&path, source).unwrap();
    let uri = lsp_types::Url::from_file_path(&path).unwrap();
    let mut client = Client::start();

    client.send(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "capabilities": {},
            "initializationOptions": {"root": root},
            "rootUri": null
        }
    }));
    let initialized = client.receive_response(1);
    assert_eq!(
        initialized["result"]["capabilities"]["signatureHelpProvider"]["triggerCharacters"],
        json!(["("])
    );
    client.send(json!({"jsonrpc": "2.0", "method": "initialized", "params": {}}));
    client.send(json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didOpen",
        "params": {
            "textDocument": {"uri": uri, "languageId": "wesl", "version": 1, "text": source}
        }
    }));
    client.receive_diagnostics(&uri);

    let mut signature_at = |id: i64, offset: usize| {
        client.send(json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "textDocument/signatureHelp",
            "params": {
                "textDocument": {"uri": uri},
                "position": position(source, offset)
            }
        }));
        client.receive_response(id)
    };

    // Builtin, cursor on the third argument.
    let third = source.find("0.0, 1.0)").unwrap() + "0.0, ".len();
    let builtin = signature_at(2, third);
    assert_eq!(builtin["result"]["activeParameter"], 2, "{builtin:#?}");
    let label = builtin["result"]["signatures"]
        [builtin["result"]["activeSignature"].as_u64().unwrap() as usize]["label"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(label.contains("clamp"), "{builtin:#?}");

    // User function, cursor on the second argument.
    let roughness = source.find("0.5)").unwrap();
    let user = signature_at(3, roughness);
    assert_eq!(user["result"]["activeParameter"], 1, "{user:#?}");
    assert_eq!(
        user["result"]["signatures"][0]["label"],
        "fn shade(albedo: vec3<f32>, roughness: f32) -> f32"
    );
    // Parameter labels are offsets into the label, not substrings.
    let offsets = &user["result"]["signatures"][0]["parameters"][1]["label"];
    let expected_start = "fn shade(albedo: vec3<f32>, ".len();
    assert_eq!(offsets[0], expected_start as u64, "{user:#?}");

    // Struct constructor, rebuilt from members.
    let constructor = source.find("vec3(0.0), 1.0)").unwrap() + "vec3(0.0), ".len();
    let structure = signature_at(4, constructor);
    assert_eq!(
        structure["result"]["signatures"][0]["label"], "Camera(origin: vec3<f32>, focal: f32)",
        "{structure:#?}"
    );
    assert_eq!(structure["result"]["activeParameter"], 1);

    client.shutdown();
}

#[test]
fn inlay_hints_render_types_and_parameter_names() {
    let temp = tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let path = root.join("main.wesl");
    let source = concat!(
        "fn shade(albedo: f32, roughness: f32) -> f32 { return albedo * roughness; }\n",
        "fn main() {\n",
        "    let tint = 0.5;\n",
        "    let lit = shade(tint, 0.25);\n",
        "}\n",
    );
    fs::write(&path, source).unwrap();
    let uri = lsp_types::Url::from_file_path(&path).unwrap();
    let mut client = Client::start();

    client.send(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "capabilities": {},
            "initializationOptions": {"root": root},
            "rootUri": null
        }
    }));
    let initialized = client.receive_response(1);
    assert!(
        initialized["result"]["capabilities"]["inlayHintProvider"] != json!(null),
        "{initialized:#?}"
    );
    client.send(json!({"jsonrpc": "2.0", "method": "initialized", "params": {}}));
    client.send(json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didOpen",
        "params": {
            "textDocument": {"uri": uri, "languageId": "wesl", "version": 1, "text": source}
        }
    }));
    client.receive_diagnostics(&uri);

    client.send(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "textDocument/inlayHint",
        "params": {
            "textDocument": {"uri": uri},
            "range": {
                "start": position(source, 0),
                "end": position(source, source.len())
            }
        }
    }));
    let hints = client.receive_response(2);
    let rendered = hints["result"].as_array().unwrap();
    assert!(
        rendered
            .iter()
            .any(|hint| hint["label"] == ": f32" && hint["kind"] == 1),
        "type hint: {hints:#?}"
    );
    assert!(
        rendered
            .iter()
            .any(|hint| hint["label"] == "albedo:" && hint["kind"] == 2),
        "parameter hint: {hints:#?}"
    );

    // The `tint` type hint must land immediately after the name.
    let tint_end = source.find("tint =").unwrap() + "tint".len();
    assert!(
        rendered
            .iter()
            .any(|hint| hint["position"] == position(source, tint_end) && hint["label"] == ": f32"),
        "{hints:#?}"
    );

    client.shutdown();
}

#[test]
fn incremental_changes_are_applied_in_order() {
    let temp = tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let path = root.join("main.wesl");
    let source = "fn main() {\n    let value: f32 = 1.0;\n}\n";
    fs::write(&path, source).unwrap();
    let uri = lsp_types::Url::from_file_path(&path).unwrap();
    let mut client = Client::start();

    client.send(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "capabilities": {},
            "initializationOptions": {"root": root},
            "rootUri": null
        }
    }));
    let initialized = client.receive_response(1);
    let sync = &initialized["result"]["capabilities"]["textDocumentSync"];
    assert_eq!(sync["change"], 2, "incremental sync: {initialized:#?}");
    assert_eq!(sync["save"]["includeText"], true, "{initialized:#?}");
    client.send(json!({"jsonrpc": "2.0", "method": "initialized", "params": {}}));
    client.send(json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didOpen",
        "params": {
            "textDocument": {"uri": uri, "languageId": "wesl", "version": 1, "text": source}
        }
    }));
    assert_eq!(
        client.receive_diagnostics(&uri)["params"]["diagnostics"],
        json!([])
    );

    // Two ranged edits in one notification: retype `f32` as `bool`, then widen the literal.
    // The second edit's range is expressed against the text left by the first.
    let type_start = source.find("f32").unwrap();
    client.send(json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didChange",
        "params": {
            "textDocument": {"uri": uri, "version": 2},
            "contentChanges": [
                {
                    "range": {
                        "start": position(source, type_start),
                        "end": position(source, type_start + "f32".len())
                    },
                    "text": "bool"
                },
                {
                    "range": {
                        "start": {"line": 1, "character": 22},
                        "end": {"line": 1, "character": 25}
                    },
                    "text": "2.0"
                }
            ]
        }
    }));

    let diagnostics = client.receive_diagnostics(&uri);
    let reported = diagnostics["params"]["diagnostics"].as_array().unwrap();
    assert_eq!(reported.len(), 1, "{diagnostics:#?}");
    assert_eq!(
        reported[0]["message"], "type mismatch: expected bool, found f32",
        "both edits must have landed: {diagnostics:#?}"
    );

    // Reverting via a ranged edit clears the diagnostic again.
    client.send(json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didChange",
        "params": {
            "textDocument": {"uri": uri, "version": 3},
            "contentChanges": [{
                "range": {
                    "start": {"line": 1, "character": 15},
                    "end": {"line": 1, "character": 19}
                },
                "text": "f32"
            }]
        }
    }));
    assert_eq!(
        client.receive_diagnostics(&uri)["params"]["diagnostics"],
        json!([]),
        "reverting the type should clear the mismatch"
    );

    client.shutdown();
}

#[test]
fn pull_capable_clients_get_reports_instead_of_pushes() {
    let temp = tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let dependency = root.join("dependency.wesl");
    fs::write(&dependency, "fn broken() { let value: bool = 1.0; }\n").unwrap();
    let path = root.join("main.wesl");
    let source = "import package::dependency::broken;\nfn main() { broken(); }\n";
    fs::write(&path, source).unwrap();
    let uri = lsp_types::Url::from_file_path(&path).unwrap();
    let mut client = Client::start();

    // Declaring pull support flips the server off the push path.
    client.send(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "capabilities": {"textDocument": {"diagnostic": {"dynamicRegistration": false}}},
            "initializationOptions": {"root": root, "diagnostics": {"pull": true}},
            "rootUri": null
        }
    }));
    let initialized = client.receive_response(1);
    assert_eq!(
        initialized["result"]["capabilities"]["diagnosticProvider"]["interFileDependencies"], true,
        "{initialized:#?}"
    );
    client.send(json!({"jsonrpc": "2.0", "method": "initialized", "params": {}}));
    client.send(json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didOpen",
        "params": {
            "textDocument": {"uri": uri, "languageId": "wesl", "version": 1, "text": source}
        }
    }));

    client.send(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "textDocument/diagnostic",
        "params": {"textDocument": {"uri": uri}}
    }));
    let report = client.receive_response(2);
    assert_eq!(report["result"]["kind"], "full", "{report:#?}");
    assert_eq!(report["result"]["items"], json!([]), "{report:#?}");

    // The failing dependency rides along in relatedDocuments.
    let dependency_uri = lsp_types::Url::from_file_path(&dependency).unwrap();
    let related = &report["result"]["relatedDocuments"][dependency_uri.as_str()];
    assert_eq!(
        related["items"][0]["message"], "type mismatch: expected bool, found f32",
        "{report:#?}"
    );

    // Nothing should have been pushed: the only traffic was our own response.
    client.send(json!({"jsonrpc": "2.0", "id": 3, "method": "shutdown"}));
    let next = client.receive();
    assert_eq!(
        next["id"], 3,
        "a pull-capable client must not receive publishDiagnostics: {next:#?}"
    );
    client.send(json!({"jsonrpc": "2.0", "method": "exit"}));
    client.input.take();
    assert!(client.child.wait().unwrap().success());
}

#[test]
fn renaming_a_shader_returns_import_edits() {
    let temp = tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let dependency = root.join("mesh.wesl");
    fs::write(&dependency, "const value = 1;\n").unwrap();
    let path = root.join("main.wesl");
    let source = "import package::mesh::value;\nconst total = value;\n";
    fs::write(&path, source).unwrap();
    let uri = lsp_types::Url::from_file_path(&path).unwrap();
    let mut client = Client::start();

    client.send(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "capabilities": {},
            "initializationOptions": {"root": root},
            "rootUri": null
        }
    }));
    let initialized = client.receive_response(1);
    assert_eq!(
        initialized["result"]["capabilities"]["workspace"]["fileOperations"]["willRename"]["filters"]
            [0]["pattern"]["glob"],
        "**/*.{wesl,wgsl}",
        "{initialized:#?}"
    );
    client.send(json!({"jsonrpc": "2.0", "method": "initialized", "params": {}}));
    client.send(json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didOpen",
        "params": {
            "textDocument": {"uri": uri, "languageId": "wesl", "version": 1, "text": source}
        }
    }));
    client.receive_diagnostics(&uri);

    let renamed = lsp_types::Url::from_file_path(root.join("geometry.wesl")).unwrap();
    let old_uri = lsp_types::Url::from_file_path(&dependency).unwrap();
    client.send(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "workspace/willRenameFiles",
        "params": {
            "files": [{"oldUri": old_uri, "newUri": renamed}]
        }
    }));
    let edits = client.receive_response(2);
    let changes = &edits["result"]["changes"][uri.as_str()];
    assert_eq!(changes[0]["newText"], "package::geometry", "{edits:#?}");
    assert_eq!(changes[0]["range"]["start"], position(source, 7));
    assert_eq!(
        changes[0]["range"]["end"],
        position(source, 7 + "package::mesh".len())
    );

    // Renaming a file nobody imports yields no edit at all.
    let unrelated = lsp_types::Url::from_file_path(root.join("main.wesl")).unwrap();
    let unrelated_target = lsp_types::Url::from_file_path(root.join("entry.wesl")).unwrap();
    client.send(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "workspace/willRenameFiles",
        "params": {
            "files": [{"oldUri": unrelated, "newUri": unrelated_target}]
        }
    }));
    assert!(
        client.receive_response(3)["result"].is_null(),
        "no importers means no edit"
    );

    client.shutdown();
}

#[test]
fn configuration_gates_struct_layout_hints_and_updates_live() {
    let temp = tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let path = root.join("main.wesl");
    let source = "struct Sphere {\n    radius: f32,\n    position: vec3<f32>,\n}\n";
    fs::write(&path, source).unwrap();
    let uri = lsp_types::Url::from_file_path(&path).unwrap();
    let mut client = Client::start();

    client.send(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "capabilities": {"workspace": {"configuration": true}},
            "rootUri": lsp_types::Url::from_file_path(&root).unwrap()
        }
    }));
    assert_eq!(client.receive_response(1)["id"], 1);
    client.send(json!({"jsonrpc": "2.0", "method": "initialized", "params": {}}));

    // Startup settings request: opt layout hints in.
    let request = client.receive();
    assert_eq!(request["method"], "workspace/configuration");
    client.send(json!({
        "jsonrpc": "2.0",
        "id": request["id"].clone(),
        "result": [{"root": root, "inlayHints": {"structLayoutHints": true}}]
    }));

    client.send(json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didOpen",
        "params": {
            "textDocument": {"uri": uri, "languageId": "wesl", "version": 1, "text": source}
        }
    }));
    client.receive_diagnostics(&uri);

    let enabled = inlay_labels(&mut client, 2, &uri, source);
    assert!(
        enabled.iter().any(|label| label.starts_with("offset ")),
        "opted in, so layout hints should show: {enabled:#?}"
    );

    // Turn them back off without restarting the server.
    client.send(json!({
        "jsonrpc": "2.0",
        "method": "workspace/didChangeConfiguration",
        "params": {"settings": {}}
    }));
    let refresh = loop {
        let message = client.receive();
        if message["method"] == "workspace/configuration" {
            break message;
        }
    };
    client.send(json!({
        "jsonrpc": "2.0",
        "id": refresh["id"].clone(),
        "result": [{"root": root, "inlayHints": {"structLayoutHints": false}}]
    }));

    let disabled = inlay_labels(&mut client, 3, &uri, source);
    assert!(
        !disabled.iter().any(|label| label.starts_with("offset ")),
        "layout hints should be gone after the settings change: {disabled:#?}"
    );

    client.shutdown();
}

#[test]
fn renaming_a_directory_returns_import_edits() {
    let temp = tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let mesh = root.join("mesh");
    fs::create_dir(&mesh).unwrap();
    fs::write(mesh.join("surface.wesl"), "const doubled = 2.0;\n").unwrap();
    let path = root.join("main.wesl");
    let source = "import package::mesh::surface::doubled;\nconst total = doubled;\n";
    fs::write(&path, source).unwrap();
    let uri = lsp_types::Url::from_file_path(&path).unwrap();
    let mut client = Client::start();

    client.send(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "capabilities": {},
            "initializationOptions": {"root": root},
            "rootUri": null
        }
    }));
    let initialized = client.receive_response(1);
    let filters = &initialized["result"]["capabilities"]["workspace"]["fileOperations"]["willRename"]
        ["filters"];
    assert!(
        filters
            .as_array()
            .unwrap()
            .iter()
            .any(|filter| filter["pattern"]["matches"] == "folder"),
        "folder renames must be registered: {initialized:#?}"
    );
    client.send(json!({"jsonrpc": "2.0", "method": "initialized", "params": {}}));
    client.send(json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didOpen",
        "params": {
            "textDocument": {"uri": uri, "languageId": "wesl", "version": 1, "text": source}
        }
    }));
    client.receive_diagnostics(&uri);

    client.send(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "workspace/willRenameFiles",
        "params": {
            "files": [{
                "oldUri": lsp_types::Url::from_file_path(&mesh).unwrap(),
                "newUri": lsp_types::Url::from_file_path(root.join("geometry")).unwrap()
            }]
        }
    }));
    let edits = client.receive_response(2);
    let changes = &edits["result"]["changes"][uri.as_str()];
    assert_eq!(
        changes[0]["newText"], "package::geometry::surface",
        "{edits:#?}"
    );

    client.shutdown();
}

#[test]
fn utf8_capable_clients_negotiate_byte_columns() {
    let temp = tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let path = root.join("main.wesl");
    // The emoji is 4 bytes but 2 UTF-16 units, so the encodings disagree after it.
    let source = "// 😀 comment\nfn main() { let x: bool = 1.0; }\n";
    fs::write(&path, source).unwrap();
    let uri = lsp_types::Url::from_file_path(&path).unwrap();
    let mut client = Client::start();

    client.send(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "capabilities": {"general": {"positionEncodings": ["utf-8", "utf-16"]}},
            "initializationOptions": {"root": root},
            "rootUri": null
        }
    }));
    let initialized = client.receive_response(1);
    assert_eq!(
        initialized["result"]["capabilities"]["positionEncoding"], "utf-8",
        "{initialized:#?}"
    );
    client.send(json!({"jsonrpc": "2.0", "method": "initialized", "params": {}}));
    client.send(json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didOpen",
        "params": {
            "textDocument": {"uri": uri, "languageId": "wesl", "version": 1, "text": source}
        }
    }));

    // The diagnostic sits on line 1, whose columns are unaffected by the emoji; what matters
    // is that the server reports it in the encoding it agreed to.
    let diagnostics = client.receive_diagnostics(&uri);
    let reported = diagnostics["params"]["diagnostics"].as_array().unwrap();
    assert_eq!(reported.len(), 1, "{diagnostics:#?}");
    let literal = source.rfind("1.0").unwrap();
    let line_start = source.find("fn main").unwrap();
    assert_eq!(
        reported[0]["range"]["start"],
        json!({"line": 1, "character": literal - line_start}),
        "{diagnostics:#?}"
    );

    client.shutdown();
}

#[test]
fn clients_without_utf8_keep_utf16() {
    let temp = tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    fs::write(root.join("main.wesl"), "const value = 1;\n").unwrap();
    let mut client = Client::start();

    client.send(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "capabilities": {},
            "initializationOptions": {"root": root},
            "rootUri": null
        }
    }));
    let initialized = client.receive_response(1);
    assert_eq!(
        initialized["result"]["capabilities"]["positionEncoding"], "utf-16",
        "the protocol default must survive a client that says nothing: {initialized:#?}"
    );
    client.send(json!({"jsonrpc": "2.0", "method": "initialized", "params": {}}));
    client.shutdown();
}

#[test]
fn range_formatting_touches_only_the_requested_range() {
    let temp = tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let path = root.join("main.wesl");
    // Both functions are over-indented; only the second is inside the requested range.
    let source = concat!(
        "fn first() {\n",
        "      let x = 1;\n",
        "}\n",
        "fn second() {\n",
        "      let y = 2;\n",
        "}\n",
    );
    fs::write(&path, source).unwrap();
    let uri = lsp_types::Url::from_file_path(&path).unwrap();
    let mut client = Client::start();

    client.send(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "capabilities": {},
            "initializationOptions": {"root": root},
            "rootUri": null
        }
    }));
    let initialized = client.receive_response(1);
    assert_eq!(
        initialized["result"]["capabilities"]["documentRangeFormattingProvider"],
        json!(true)
    );
    client.send(json!({"jsonrpc": "2.0", "method": "initialized", "params": {}}));
    client.send(json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didOpen",
        "params": {
            "textDocument": {"uri": uri, "languageId": "wesl", "version": 1, "text": source}
        }
    }));
    client.receive_diagnostics(&uri);

    let second_body = source.find("      let y").unwrap();
    client.send(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "textDocument/rangeFormatting",
        "params": {
            "textDocument": {"uri": uri},
            "range": {
                "start": position(source, second_body),
                "end": position(source, second_body + "      let y = 2;".len())
            },
            "options": {"tabSize": 4, "insertSpaces": true}
        }
    }));
    let response = client.receive_response(2);
    let edits = response["result"].as_array().unwrap();
    assert!(!edits.is_empty(), "{response:#?}");

    // Every returned edit must sit on the requested line, leaving `first` alone.
    let requested_line = position(source, second_body)["line"].as_u64().unwrap();
    for edit in edits {
        assert_eq!(
            edit["range"]["start"]["line"].as_u64().unwrap(),
            requested_line,
            "edit outside the requested range: {response:#?}"
        );
    }
    assert!(
        edits
            .iter()
            .any(|edit| edit["newText"].as_str().unwrap().contains("    let y = 2;")),
        "{response:#?}"
    );

    client.shutdown();
}

#[test]
fn on_type_formatting_reindents_the_current_line() {
    let temp = tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let path = root.join("main.wesl");
    let source = "fn main() {\n    let x = 1;\n        }\n";
    fs::write(&path, source).unwrap();
    let uri = lsp_types::Url::from_file_path(&path).unwrap();
    let mut client = Client::start();

    client.send(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "capabilities": {},
            "initializationOptions": {"root": root},
            "rootUri": null
        }
    }));
    let initialized = client.receive_response(1);
    assert_eq!(
        initialized["result"]["capabilities"]["documentOnTypeFormattingProvider"]["firstTriggerCharacter"],
        "}"
    );
    client.send(json!({"jsonrpc": "2.0", "method": "initialized", "params": {}}));
    client.send(json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didOpen",
        "params": {
            "textDocument": {"uri": uri, "languageId": "wesl", "version": 1, "text": source}
        }
    }));
    client.receive_diagnostics(&uri);

    let brace = source.rfind('}').unwrap();
    client.send(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "textDocument/onTypeFormatting",
        "params": {
            "textDocument": {"uri": uri},
            "position": position(source, brace),
            "ch": "}",
            "options": {"tabSize": 4, "insertSpaces": true}
        }
    }));
    let response = client.receive_response(2);
    let edits = response["result"].as_array().unwrap();
    assert_eq!(edits.len(), 1, "{response:#?}");
    // The over-indentation is replaced with nothing, dedenting to column zero.
    assert_eq!(edits[0]["newText"], "", "{response:#?}");
    assert_eq!(
        edits[0]["range"]["start"],
        json!({"line": 2, "character": 0})
    );
    assert_eq!(edits[0]["range"]["end"], json!({"line": 2, "character": 8}));

    // A line that is already correct yields nothing at all.
    client.send(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "textDocument/onTypeFormatting",
        "params": {
            "textDocument": {"uri": uri},
            "position": position(source, source.find("let x").unwrap()),
            "ch": "\n",
            "options": {"tabSize": 4, "insertSpaces": true}
        }
    }));
    assert!(client.receive_response(3)["result"].is_null());

    client.shutdown();
}

/// Brings a server up on `root` with no special capabilities, returning the client.
fn start_on(root: &std::path::Path) -> Client {
    let mut client = Client::start();
    client.send(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "capabilities": {},
            "initializationOptions": {"root": root},
            "rootUri": null
        }
    }));
    assert_eq!(client.receive_response(1)["id"], 1);
    client.send(json!({"jsonrpc": "2.0", "method": "initialized", "params": {}}));
    client
}

#[test]
fn malformed_requests_fail_alone_and_the_server_keeps_serving() {
    let temp = tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let path = root.join("main.wesl");
    let source = "fn main() { let x = 1; }\n";
    fs::write(&path, source).unwrap();
    let uri = lsp_types::Url::from_file_path(&path).unwrap();
    let mut client = start_on(&root);

    client.send(json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didOpen",
        "params": {
            "textDocument": {"uri": uri, "languageId": "wesl", "version": 1, "text": source}
        }
    }));
    client.receive_diagnostics(&uri);

    // Params of the wrong shape entirely. Before handler isolation this propagated out of the
    // message loop and terminated the process.
    client.send(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "textDocument/hover",
        "params": {"nonsense": true}
    }));
    let rejected = client.receive_response(2);
    assert!(rejected["error"].is_object(), "{rejected:#?}");

    // A URI the server cannot turn into a path is likewise a request-level failure.
    client.send(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "textDocument/documentSymbol",
        "params": {"textDocument": {"uri": "untitled:Untitled-1"}}
    }));
    assert!(client.receive_response(3)["error"].is_object());

    // An unknown method is answered, not ignored.
    client.send(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "textDocument/somethingInvented",
        "params": {}
    }));
    let unknown = client.receive_response(4);
    assert_eq!(
        unknown["error"]["code"], -32601,
        "expected MethodNotFound: {unknown:#?}"
    );

    // A malformed notification must not kill the loop either.
    client.send(json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didChange",
        "params": {"garbage": []}
    }));

    // The server is still fully alive and answering correctly.
    client.send(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "textDocument/documentSymbol",
        "params": {"textDocument": {"uri": uri}}
    }));
    let symbols = client.receive_response(5);
    assert!(
        symbols["result"]
            .as_array()
            .unwrap()
            .iter()
            .any(|symbol| symbol["name"] == "main"),
        "server should still be serving: {symbols:#?}"
    );

    client.shutdown();
}

#[test]
fn requests_for_unopened_documents_are_answered_not_fatal() {
    let temp = tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    fs::write(root.join("present.wesl"), "const value = 1;\n").unwrap();
    let mut client = start_on(&root);

    // Never opened, and does not exist on disk either.
    let missing = lsp_types::Url::from_file_path(root.join("absent.wesl")).unwrap();
    for (id, method, params) in [
        (
            2,
            "textDocument/documentSymbol",
            json!({"textDocument": {"uri": missing}}),
        ),
        (
            3,
            "textDocument/hover",
            json!({"textDocument": {"uri": missing}, "position": {"line": 0, "character": 0}}),
        ),
        (
            4,
            "textDocument/foldingRange",
            json!({"textDocument": {"uri": missing}}),
        ),
        (
            5,
            "textDocument/inlayHint",
            json!({
                "textDocument": {"uri": missing},
                "range": {
                    "start": {"line": 0, "character": 0},
                    "end": {"line": 0, "character": 0}
                }
            }),
        ),
    ] {
        client.send(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        let response = client.receive_response(id);
        assert!(
            response["error"].is_null(),
            "{method} on an unopened document should succeed emptily: {response:#?}"
        );
    }

    client.shutdown();
}

#[test]
fn requests_after_shutdown_are_rejected() {
    let temp = tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    fs::write(root.join("main.wesl"), "const value = 1;\n").unwrap();
    let mut client = start_on(&root);

    client.send(json!({"jsonrpc": "2.0", "id": 2, "method": "shutdown"}));
    assert!(client.receive_response(2)["error"].is_null());

    // The protocol requires anything after shutdown to be refused.
    client.send(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "textDocument/documentSymbol",
        "params": {"textDocument": {"uri": lsp_types::Url::from_file_path(root.join("main.wesl")).unwrap()}}
    }));
    let refused = client.receive_response(3);
    assert!(
        refused["error"].is_object(),
        "post-shutdown requests must be refused: {refused:#?}"
    );

    client.send(json!({"jsonrpc": "2.0", "method": "exit"}));
    client.input.take();
    assert!(client.child.wait().unwrap().success());
}

/// Zed advertises pull diagnostics by default, but pull was tried against Zed and backed out.
/// Client capability alone must therefore not be enough to switch off the push path.
#[test]
fn advertising_pull_support_alone_does_not_disable_push() {
    let temp = tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let path = root.join("main.wesl");
    let source = "fn main() { let x: bool = 1.0; }\n";
    fs::write(&path, source).unwrap();
    let uri = lsp_types::Url::from_file_path(&path).unwrap();
    let mut client = Client::start();

    client.send(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            // Exactly what Zed sends with its default lsp_pull_diagnostics.enabled = true.
            "capabilities": {
                "textDocument": {
                    "diagnostic": {"dynamicRegistration": true, "relatedDocumentSupport": true}
                }
            },
            "initializationOptions": {"root": root},
            "rootUri": null
        }
    }));
    let initialized = client.receive_response(1);
    assert!(
        initialized["result"]["capabilities"]
            .get("diagnosticProvider")
            .is_none(),
        "pull must stay opt-in even when the client advertises it: {initialized:#?}"
    );
    client.send(json!({"jsonrpc": "2.0", "method": "initialized", "params": {}}));
    client.send(json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didOpen",
        "params": {
            "textDocument": {"uri": uri, "languageId": "wesl", "version": 1, "text": source}
        }
    }));

    // And diagnostics must still arrive unprompted.
    let published = client.receive_diagnostics(&uri);
    assert_eq!(
        published["params"]["diagnostics"][0]["message"], "type mismatch: expected bool, found f32",
        "{published:#?}"
    );

    client.shutdown();
}

// ---------------------------------------------------------------------------------------------
// Real client capability fixtures.
//
// Captured verbatim from each client's source on 2026-07-25. The Zed pull-diagnostics
// regression happened because these were assumed rather than checked, so they are frozen here
// and the behaviour each one produces is asserted. If a client changes what it sends, refresh
// the fixture deliberately and look at what the assertions do.
// ---------------------------------------------------------------------------------------------

/// zed-industries/zed, crates/lsp/src/lsp.rs `default_initialize_params`.
/// `textDocument.diagnostic` is present because `lsp_pull_diagnostics.enabled` defaults to true.
fn zed_capabilities() -> Value {
    json!({
        "general": {"positionEncodings": ["utf-16"]},
        "workspace": {
            "configuration": true,
            "didChangeConfiguration": {"dynamicRegistration": true},
            "workspaceFolders": true,
            "diagnostics": {"refreshSupport": true}
        },
        "textDocument": {
            "diagnostic": {"dynamicRegistration": true, "relatedDocumentSupport": true},
            "documentSymbol": {"hierarchicalDocumentSymbolSupport": true},
            "foldingRange": {"lineFoldingOnly": false}
        }
    })
}

/// neovim/neovim, runtime/lua/vim/lsp/protocol.lua `make_client_capabilities`.
fn neovim_capabilities() -> Value {
    json!({
        "general": {"positionEncodings": ["utf-8", "utf-16", "utf-32"]},
        "workspace": {"configuration": true, "workspaceFolders": true},
        "textDocument": {"documentSymbol": {"hierarchicalDocumentSymbolSupport": true}}
    })
}

/// Brings a server up with the given capabilities, answering the startup settings request that
/// any client advertising `workspace.configuration` triggers.
fn initialize_with(capabilities: Value, root: &std::path::Path) -> (Client, Value) {
    let mut client = Client::start();
    client.send(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "capabilities": capabilities,
            "initializationOptions": {"root": root},
            "rootUri": null
        }
    }));
    let initialized = client.receive_response(1);
    client.send(json!({"jsonrpc": "2.0", "method": "initialized", "params": {}}));
    let request = client.receive();
    assert_eq!(
        request["method"], "workspace/configuration",
        "both fixtures advertise workspace.configuration: {request:#?}"
    );
    client.send(json!({
        "jsonrpc": "2.0",
        "id": request["id"].clone(),
        "result": [{"root": root}]
    }));
    (client, initialized)
}

#[test]
fn zed_gets_push_diagnostics_and_utf16() {
    let temp = tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let path = root.join("main.wesl");
    let source = "fn main() { let x: bool = 1.0; }\n";
    fs::write(&path, source).unwrap();
    let uri = lsp_types::Url::from_file_path(&path).unwrap();

    let (mut client, initialized) = initialize_with(zed_capabilities(), &root);
    let capabilities = &initialized["result"]["capabilities"];

    assert_eq!(capabilities["positionEncoding"], "utf-16");
    assert!(
        capabilities.get("diagnosticProvider").is_none(),
        "Zed advertises pull but it was backed out for Zed; push must survive: {initialized:#?}"
    );
    assert_eq!(
        capabilities["workspace"]["workspaceFolders"]["supported"],
        true
    );

    // And diagnostics must actually arrive without being asked for.
    client.send(json!({
        "jsonrpc": "2.0",
        "method": "textDocument/didOpen",
        "params": {
            "textDocument": {"uri": uri, "languageId": "wesl", "version": 1, "text": source}
        }
    }));
    let published = client.receive_diagnostics(&uri);
    assert_eq!(
        published["params"]["diagnostics"][0]["message"],
        "type mismatch: expected bool, found f32"
    );

    client.shutdown();
}

#[test]
fn neovim_gets_utf8_columns() {
    let temp = tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    fs::write(root.join("main.wesl"), "const value = 1;\n").unwrap();

    let (mut client, initialized) = initialize_with(neovim_capabilities(), &root);
    assert_eq!(
        initialized["result"]["capabilities"]["positionEncoding"], "utf-8",
        "Neovim offers utf-8 first and analysis is byte-native: {initialized:#?}"
    );
    // Neovim does not advertise pull diagnostics at all, so push is the only option.
    assert!(
        initialized["result"]["capabilities"]
            .get("diagnosticProvider")
            .is_none()
    );

    client.shutdown();
}
