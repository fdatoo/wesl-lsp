//! A transparent proxy that records an editor's real LSP conversation with `wesl-lsp`.
//!
//! Point an editor at this instead of the server and it relays both directions byte for byte
//! while appending every message to a trace file. Real sessions then become replay tests, so
//! time spent using the editor compounds into permanent regression coverage instead of being a
//! one-time observation — see `tests/trace_replay.rs`.
//!
//! ```text
//! WESL_LSP_TRACE=/tmp/session.jsonl wesl-lsp-record
//! ```
//!
//! `WESL_LSP_SERVER` overrides the server binary, which otherwise defaults to `wesl-lsp` beside
//! this one. Traces contain the file paths and shader source of whoever recorded them, so they
//! are not committed.

use std::{
    env,
    fs::OpenOptions,
    io::{self, BufRead, BufReader, Write},
    path::PathBuf,
    process::{Command, Stdio},
    sync::{Arc, Mutex},
    thread,
    time::Instant,
};

fn main() -> io::Result<()> {
    let trace_path = env::var_os("WESL_LSP_TRACE")
        .map(PathBuf::from)
        .ok_or_else(|| {
            io::Error::other("set WESL_LSP_TRACE to the file the session should be recorded to")
        })?;
    let server = env::var_os("WESL_LSP_SERVER")
        .map(PathBuf::from)
        .or_else(|| {
            let mut sibling = env::current_exe().ok()?;
            sibling.set_file_name(if cfg!(windows) {
                "wesl-lsp.exe"
            } else {
                "wesl-lsp"
            });
            sibling.exists().then_some(sibling)
        })
        .ok_or_else(|| io::Error::other("no wesl-lsp beside this binary; set WESL_LSP_SERVER"))?;

    let trace = Arc::new(Mutex::new(
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(&trace_path)?,
    ));
    let started = Instant::now();

    let mut child = Command::new(&server)
        .args(env::args_os().skip(1))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;
    let mut to_server = child.stdin.take().expect("piped");
    let from_server = BufReader::new(child.stdout.take().expect("piped"));

    // Server to editor, on its own thread so neither direction can block the other.
    let server_trace = Arc::clone(&trace);
    let pump = thread::spawn(move || {
        let mut from_server = from_server;
        let mut out = io::stdout().lock();
        while let Some(body) = read_message(&mut from_server)? {
            record(&server_trace, started, "server", &body);
            write_message(&mut out, &body)?;
        }
        io::Result::Ok(())
    });

    // Editor to server, on this thread.
    let mut stdin = BufReader::new(io::stdin().lock());
    while let Some(body) = read_message(&mut stdin)? {
        record(&trace, started, "client", &body);
        write_message(&mut to_server, &body)?;
    }
    // Closing stdin lets the server finish, which ends the pump.
    drop(to_server);
    let _ = pump.join();
    let _ = child.wait();
    Ok(())
}

/// One JSON object per line: elapsed milliseconds, direction, and the message verbatim.
fn record(trace: &Mutex<std::fs::File>, started: Instant, from: &str, body: &[u8]) {
    let message = serde_json::from_slice::<serde_json::Value>(body)
        .unwrap_or_else(|_| serde_json::Value::String(String::from_utf8_lossy(body).into_owned()));
    let line = serde_json::json!({
        "at_ms": started.elapsed().as_millis() as u64,
        "from": from,
        "message": message,
    });
    if let Ok(mut trace) = trace.lock() {
        let _ = writeln!(trace, "{line}");
        let _ = trace.flush();
    }
}

fn read_message(reader: &mut impl BufRead) -> io::Result<Option<Vec<u8>>> {
    let mut content_length = None;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            return Ok(None);
        }
        if line == "\r\n" || line == "\n" {
            break;
        }
        if let Some(value) = line
            .strip_prefix("Content-Length:")
            .or_else(|| line.strip_prefix("content-length:"))
        {
            content_length = value.trim().parse::<usize>().ok();
        }
    }
    let Some(length) = content_length else {
        return Ok(None);
    };
    let mut body = vec![0; length];
    reader.read_exact(&mut body)?;
    Ok(Some(body))
}

fn write_message(writer: &mut impl Write, body: &[u8]) -> io::Result<()> {
    write!(writer, "Content-Length: {}\r\n\r\n", body.len())?;
    writer.write_all(body)?;
    writer.flush()
}
