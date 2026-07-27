# wesl-lsp

A language server for [WESL](https://wesl-lang.dev) and WGSL shaders.

WESL extends WGSL with an import system, so a shader is no longer a single flat file. That is
what this server is built around: imports resolve across files, types flow across module
boundaries, and a fix in one shader clears stale errors in everything that imports it.

The guiding invariant is **no false diagnostics on valid shaders**. Nobody keeps a language
server that lights up correct code, so the test suite is built to prove the absence of false
positives rather than the presence of features — see [Testing](#testing).

## Features

| | |
|---|---|
| **Diagnostics** | Parse errors, unresolved imports, import cycles, link errors, type errors — in that order, each stage gated on the previous being clean, so one broken import doesn't produce an avalanche of downstream noise. Push or pull. |
| **Navigation** | Go-to-definition, find references, document/workspace symbols, document highlight. |
| **Editing** | Rename (with prepare), completion (trigger `.`) with auto-import, signature help (trigger `(`, retrigger `,`), folding ranges, selection ranges. |
| **Hover** | Signatures and doc comments, with types inferred across imports. |
| **Inlay hints** | Inferred types, parameter names, and struct memory layout (byte offset, alignment, size). |
| **Formatting** | Whole document, range, and on-type. |
| **Refactoring** | `willRenameFiles` rewrites import paths when a shader or a directory of shaders is renamed. |
| **File watching** | On-disk changes made outside the editor — a branch switch, a `git pull`, a generated shader — keep the index current. |

Both `.wesl` and `.wgsl` are supported, as are naga_oil-dialect files (`#import`, `#ifdef`,
`#{...}`). Dialect files are indexed for navigation but get no type diagnostics, because their
meaning depends on preprocessor conditionals the server cannot resolve.

Not yet implemented: semantic tokens, code actions, code lens, call hierarchy.

## Installing

Pre-built binaries for macOS (arm64/x86_64), Linux (arm64/x86_64) and Windows x86_64 are
attached to each [release](https://github.com/fdatoo/wesl-lsp/releases).

From source (Rust 1.96 or newer):

```sh
cargo install --git https://github.com/fdatoo/wesl-lsp wesl-lsp
```

## Editor setup

The server speaks LSP over stdio; run the `wesl-lsp` binary with no arguments. Point your
editor at it for the `wesl` and `wgsl` languages.

Neovim, using the built-in LSP client:

```lua
vim.filetype.add({ extension = { wesl = "wesl", wgsl = "wgsl" } })

vim.lsp.config["wesl_lsp"] = {
  cmd = { "wesl-lsp" },
  filetypes = { "wesl", "wgsl" },
  root_markers = { "wesl.toml", ".git" },
  settings = {
    ["wesl-lsp"] = {
      inlayHints = { structLayoutHints = true },
    },
  },
}
vim.lsp.enable("wesl_lsp")
```

Settings live under the `wesl-lsp` section and are also accepted verbatim as
`initializationOptions`, so clients that cannot answer `workspace/configuration` can still
configure the server. Every field is optional and a partial object leaves the rest at default.

| Setting | Default | |
|---|---|---|
| `root` | discovered | Overrides root discovery entirely. |
| `inlayHints.enabled` | `true` | |
| `inlayHints.typeHints` | `true` | `let x: f32` on declarations with no written type. |
| `inlayHints.parameterHints` | `true` | `clamp(low: 0.0)` at call sites. |
| `inlayHints.structLayoutHints` | `false` | Byte offset, alignment and size per struct member. |
| `diagnostics.enabled` | `true` | |
| `diagnostics.pull` | `false` | See below. |

Layout hints default off deliberately: they annotate every member of every struct with roughly
28 characters of virtual text regardless of what you are doing, and the information is only
actionable while reconciling a shader struct against a host-side one. Turn them on for that,
then turn them back off.

**Diagnostics use exactly one mechanism per session.** Pushing is the default. Pull requires
both that the client advertise `textDocument/diagnostic` *and* that `diagnostics.pull` be set —
client capability alone is deliberately not enough, because Zed advertises pull support by
default and pull was tried against Zed and backed out. Advertising both would double-report in
clients that do both, which the specification advises against.

Without an explicit `root`, the server discovers one per shader: a `wesl.toml` marker wins
outright, otherwise it walks upward through contiguous directories containing shaders and stops
at the first that doesn't. The client's `workspaceFolders` are used as roots when provided.

## Development

```sh
cargo build --workspace
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings   # CI gate
cargo fmt --all                                                   # CI gate: --check
```

Run the server by hand with logging:

```sh
RUST_LOG=debug cargo run -p wesl-lsp -- --log-timing
```

### The corpus is required before tests mean anything

`corpus/` is gitignored and every corpus test returns early when its input is missing, so a
green `cargo test` on a fresh checkout has verified almost nothing. Fetch the pinned corpora
first — sparse shallow checkouts of wesl-rs, wgpu, webgpu-samples and bevy, pinned by commit
SHA in `xtask/src/main.rs`:

```sh
cargo run -p xtask -- fetch-corpus
```

`WESL_LSP_REQUIRE_CORPORA=1` turns a missing corpus from a skip into a failure, and CI sets it.
Set it locally too. Changing a corpus pin changes the meaning of every corpus test, so expect to
re-audit the known-divergence lists in `differential.rs` when you do.

Two corpora stay opt-in because CI has neither:

```sh
WESL_LSP_PRIVATE_CORPUS=/path/to/shaders \
WESL_LSP_TRACES=/path/to/traces \
WESL_LSP_REQUIRE_CORPORA=1 \
  cargo test --workspace
```

A trace is a recording of a real editor session. `wesl-lsp-record` is a transparent proxy —
point your editor at it instead of the server and it relays both directions while appending
every message to a file, which `trace_replay.rs` then drives a fresh server from:

```sh
WESL_LSP_TRACE=~/wesl-traces/session.jsonl wesl-lsp-record
```

Traces contain the recorder's absolute paths and shader source, so they are never committed.

### Testing

| | |
|---|---|
| `differential.rs` | **naga as an oracle**, asserting *bidirectional* agreement over the corpus: this checker errors if and only if naga does. Deviation in either direction fails. |
| `public_corpus.rs` / `private_corpus.rs` | Pure false-positive gates: valid shaders, zero diagnostics. |
| `span_fidelity.rs` | Every span the parser reports slices to the exact expected source text. Span drift turns a correct diagnostic into a squiggle under the wrong token. |
| `editor_services_corpus.rs` | Every offset-based service over every corpus shader — no panics, no structurally invalid results. A panic in any handler kills the server. |
| `mutation_fuzz.rs` | The same services over *deliberately damaged* shaders. The corpus is valid, but a buffer is broken for most of the time anyone is typing in it, which is exactly when completion and signature help get called. Seeded, so a failure names a reproducing shader and seed. |
| `lsp_smoke.rs` | Spawns the real binary and speaks JSON-RPC over stdio — the only test covering protocol wiring. Carries frozen capability fixtures for Zed and Neovim, captured verbatim from their sources. |
| `main.rs` unit tests | Property-tests incremental sync over hundreds of pseudo-random edit batches in both position encodings. A misapplied range desynchronises the buffer with no error anywhere, and every later answer is computed against the wrong text. |
| `layout.rs` | Struct layout checked against naga, which lays out real GPU buffers, on every struct in the corpus. |

Several of these were validated by fault injection — the real bug was re-introduced and the test
confirmed to catch it. That is the bar for a test here: it earns its keep only if it can fail.

## Architecture

`wesl-lsp` knows the protocol and nothing else. `wesl-analysis` holds all the semantics and
knows nothing about LSP — it speaks **byte offsets**, not line/character, with `LineIndex`
converting at the boundary. `wesl-fmt` formats, and refuses to act when unsure: it returns
nothing unless the output reparses, round-trips identically, and is a fixed point, which leaves
the file untouched rather than mangled.

Parsing and compilation come from [wesl-rs](https://github.com/wgsl-tooling-wg/wesl-rs).

[`CLAUDE.md`](CLAUDE.md) documents the internals in depth — the diagnostic cascade, the
last-good-AST and length-preserving-preprocessing designs, root discovery, and the reasoning
behind each capability decision.

## License

MIT — see [LICENSE-MIT](LICENSE-MIT).
