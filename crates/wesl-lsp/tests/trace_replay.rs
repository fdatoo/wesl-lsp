//! Replays real editor sessions recorded by `wesl-lsp-record` against a fresh server.
//!
//! Scripted smoke tests only cover conversations someone thought to write down. A trace is a
//! conversation a real client actually had, so replaying it exercises orderings, capability
//! combinations and request shapes nobody imagined.
//!
//! ```sh
//! WESL_LSP_TRACES=~/wesl-traces cargo test -p wesl-lsp --test trace_replay
//! ```
//!
//! **This test skips silently when `WESL_LSP_TRACES` is unset**, like the private corpus gates.
//! A green run proves nothing unless you set it.
//!
//! What it asserts is survival and completeness, not identical output: traces carry the
//! recorder's absolute paths, so results legitimately differ on another machine. Every request
//! must still get a response, and the server must exit cleanly.

use std::{
    fs,
    io::{BufRead, BufReader, Write},
    path::PathBuf,
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc,
    thread,
    time::Duration,
};

use serde_json::{Value, json};

/// Long enough for a cold package index on a large workspace, short enough that a hung server
/// fails the run instead of stalling CI forever.
const REPLY_TIMEOUT: Duration = Duration::from_secs(30);

fn traces() -> Vec<(PathBuf, Vec<Value>)> {
    let Some(directory) = std::env::var_os("WESL_LSP_TRACES").map(PathBuf::from) else {
        return Vec::new();
    };
    let mut traces = fs::read_dir(&directory)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|e| e.to_str()) == Some("jsonl"))
        .filter_map(|path| {
            let events = fs::read_to_string(&path)
                .ok()?
                .lines()
                .filter_map(|line| serde_json::from_str::<Value>(line).ok())
                .collect::<Vec<_>>();
            Some((path, events))
        })
        .collect::<Vec<_>>();
    traces.sort_by(|left, right| left.0.cmp(&right.0));
    traces
}

fn write_message(stdin: &mut ChildStdin, message: &Value) {
    let body = serde_json::to_vec(message).unwrap();
    write!(stdin, "Content-Length: {}\r\n\r\n", body.len()).unwrap();
    stdin.write_all(&body).unwrap();
    stdin.flush().unwrap();
}

fn read_message(reader: &mut impl BufRead) -> Option<Value> {
    let mut content_length = None;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).ok()? == 0 {
            return None;
        }
        if line == "\r\n" {
            break;
        }
        if let Some(value) = line.strip_prefix("Content-Length: ") {
            content_length = value.trim().parse::<usize>().ok();
        }
    }
    let mut body = vec![0; content_length?];
    std::io::Read::read_exact(reader, &mut body).ok()?;
    serde_json::from_slice(&body).ok()
}

struct Replay {
    child: Child,
    stdin: Option<ChildStdin>,
    incoming: mpsc::Receiver<Value>,
}

impl Replay {
    fn start() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_wesl-lsp"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        // Reading on a thread keeps a wedged server from hanging the whole run.
        let (sender, incoming) = mpsc::channel();
        thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            while let Some(message) = read_message(&mut reader) {
                if sender.send(message).is_err() {
                    break;
                }
            }
        });
        Self {
            child,
            stdin: Some(stdin),
            incoming,
        }
    }
}

impl Drop for Replay {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

#[test]
fn recorded_sessions_replay_without_losing_the_server() {
    let traces = traces();
    if traces.is_empty() {
        return;
    }

    for (path, events) in &traces {
        let label = path.display();
        let mut replay = Replay::start();
        let mut awaiting: Vec<Value> = Vec::new();
        let mut answered = 0;

        for event in events {
            // Server-side events are the recording, not the script; we drive from the client.
            if event["from"] != "client" {
                continue;
            }
            let message = &event["message"];
            if message["method"].is_null() && message.get("id").is_some() {
                // A recorded reply to a server request; the live server will ask again itself.
                continue;
            }
            if let Some(id) = message.get("id")
                && !id.is_null()
            {
                awaiting.push(id.clone());
            }
            write_message(replay.stdin.as_mut().unwrap(), message);

            // Drain whatever the server has to say, answering any request it makes of us.
            while let Ok(incoming) = replay.incoming.recv_timeout(Duration::from_millis(150)) {
                if incoming.get("method").is_some() && incoming.get("id").is_some() {
                    // `workspace/configuration` and friends: a null result leaves defaults.
                    write_message(
                        replay.stdin.as_mut().unwrap(),
                        &json!({"jsonrpc": "2.0", "id": incoming["id"].clone(), "result": null}),
                    );
                } else if let Some(id) = incoming.get("id")
                    && let Some(index) = awaiting.iter().position(|pending| pending == id)
                {
                    awaiting.remove(index);
                    answered += 1;
                }
            }
        }

        // Anything still outstanding gets one more chance before we call it a hang.
        while !awaiting.is_empty() {
            let Ok(incoming) = replay.incoming.recv_timeout(REPLY_TIMEOUT) else {
                panic!("{label}: server never answered {awaiting:?}");
            };
            if let Some(id) = incoming.get("id")
                && incoming.get("method").is_none()
                && let Some(index) = awaiting.iter().position(|pending| pending == id)
            {
                awaiting.remove(index);
                answered += 1;
            }
        }

        replay.stdin.take();
        let status = replay.child.wait().unwrap();
        assert!(status.success(), "{label}: server exited with {status}");
        assert!(answered > 0, "{label}: trace drove no requests at all");
        eprintln!("{label}: replayed, {answered} request(s) answered");
    }
}
