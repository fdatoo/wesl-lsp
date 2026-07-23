use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    path::PathBuf,
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
};

use serde_json::{Value, json};

struct Client {
    child: Child,
    input: Option<ChildStdin>,
    output: BufReader<ChildStdout>,
}

impl Client {
    fn start() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_wesl-lsp"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let input = child.stdin.take().unwrap();
        let output = BufReader::new(child.stdout.take().unwrap());
        Self {
            child,
            input: Some(input),
            output,
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
        let mut content_length = None;
        loop {
            let mut line = String::new();
            assert_ne!(
                self.output.read_line(&mut line).unwrap(),
                0,
                "server closed stdout"
            );
            if line == "\r\n" {
                break;
            }
            if let Some(length) = line.strip_prefix("Content-Length: ") {
                content_length = Some(length.trim().parse::<usize>().unwrap());
            }
        }
        let mut body = vec![0; content_length.unwrap()];
        self.output.read_exact(&mut body).unwrap();
        serde_json::from_slice(&body).unwrap()
    }

    fn shutdown(&mut self) {
        eprintln!("sending shutdown");
        self.send(json!({"jsonrpc": "2.0", "id": 99, "method": "shutdown"}));
        let response = self.receive();
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

fn shaders() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../Seclorum/novus/crates/novus-render/shaders")
}

fn position(source: &str, offset: usize) -> Value {
    let index = wesl_analysis::LineIndex::new(source);
    let position = index.offset_to_position(source, offset).unwrap();
    json!({"line": position.line, "character": position.character})
}

#[test]
fn publishes_clean_then_isolated_import_diagnostics() {
    let shader_root = shaders();
    if !shader_root.is_dir() {
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
    let initialized = client.receive();
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
    let clean = client.receive();
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
    let diagnostics = client.receive();
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
        client.receive()["params"]["diagnostics"],
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
    let definition = client.receive();
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
    let references = client.receive();
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
    let rename = client.receive();
    assert_eq!(rename["id"], 4);
    assert!(rename["result"]["changes"].as_object().unwrap().len() >= 4);

    client.send(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "textDocument/documentSymbol",
        "params": {"textDocument": {"uri": clouds_uri}}
    }));
    let symbols = client.receive();
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
    let hover = client.receive();
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
    let completion = client.receive();
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
    let type_diagnostics = client.receive();
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
    let blit_diagnostics = client.receive();
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
    let formatted = client.receive();
    assert_eq!(formatted["id"], 8);
    let formatted = formatted["result"][0]["newText"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(formatted.contains("\n  @builtin(position) clip_position"));
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
    let idempotent = client.receive();
    assert_eq!(idempotent["id"], 9);
    assert!(idempotent["result"].is_null());

    eprintln!("diagnostics verified");
    client.shutdown();
}
