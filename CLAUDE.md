# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

A language server for WESL and WGSL shaders. `wesl-lsp` is the binary; `wesl-analysis` holds
all the semantics; `wesl-fmt` formats. Rust workspace, edition 2024, MSRV 1.96.

## Commands

```sh
# Fetch the pinned shader corpora — REQUIRED before tests mean anything (see below)
cargo run -p xtask -- fetch-corpus

cargo build --workspace
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings   # CI gate
cargo fmt --all                                                   # CI gate: --check

# A single integration test file, then a single test within it
cargo test -p wesl-analysis --test differential
cargo test -p wesl-analysis --test differential public_corpus_matches_naga_bidirectionally

# A single unit test (they live in `mod tests` inside src/)
cargo test -p wesl-analysis analysis::tests::cyclic_import_is_reported_once

# Run the server by hand
RUST_LOG=debug cargo run -p wesl-lsp -- --log-timing

# Regenerate builtins.rs from a wgsl-spec functions.json
cargo run -p xtask -- generate-builtins <functions.json>
```

### Tests silently skip when their inputs are missing

This is the single most important thing to know about this repo. `corpus/` is gitignored, and
every corpus test begins with `if !root.exists() { return; }`. The private-corpus tests return
early unless `WESL_LSP_PRIVATE_CORPUS` points at a directory of `.wesl` files, and
`span_fidelity` needs a specific private shader. **A green `cargo test` on a fresh checkout has
verified almost nothing.** Run `fetch-corpus` first, and treat "0 failures" as meaningless until
you have confirmed the corpus is present.

The guard assertions (`compared >= 100`, `formatted_count >= 100`, "oracle did not exercise both
error paths") exist precisely to catch a corpus that fetched but came up empty. Don't weaken them.

```sh
WESL_LSP_PRIVATE_CORPUS=/path/to/shaders cargo test --workspace
```

## Architecture

### Layering

`wesl-lsp` knows the protocol and nothing else. `wesl-analysis` knows nothing about LSP — it
speaks **byte offsets**, not line/character. `LineIndex` converts at the boundary, in
`crates/wesl-lsp/src/main.rs` only. Keep it that way: no `lsp_types` in the analysis crate.

Parsing and compilation come from `wesl-rs` (`wesl`, `wgsl-parse`, `wgsl-types`), pinned to a git
rev of the `fdatoo/wesl-rs` fork. All three are pinned to the *same* rev in the workspace
`Cargo.toml` — bump them together.

### AnalysisHost (`analysis.rs`)

Owns `documents` (open editor buffers) and `packages` (a `PackageIndex` per discovered root).

Two design decisions drive most of the behavior:

**Last-good AST.** `Document::parse` retains the previous successful `TranslationUnit` when the
current text fails to parse. Completions, hover, and go-to-definition keep working while the user
is mid-keystroke. `PackageIndex::update` does the same — on a parse failure it swaps in the new
source but keeps the stale index.

**Length-preserving preprocessing.** naga_oil files (`#import`, `#ifdef`, `#{...}`) are detected
by `dialect::is_naga_oil` and rewritten by `dialect::preprocess`, which blanks directives to
spaces and substitutes equal-length identifiers for `#{...}`. Byte offsets in the preprocessed
text therefore match the original exactly, so every span the parser produces is still valid
against the real source. Anything you add to `dialect.rs` must preserve length.

Dialect files get **no type diagnostics** (`diagnostics` returns early) because their meaning
depends on preprocessor conditionals, but they are still indexed for navigation via `oil_imports`
and `oil_definitions`.

### The diagnostic cascade

`AnalysisHost::diagnostics` is a sequence of stages, each gated on the previous producing nothing:

1. parse error → one diagnostic, return
2. dialect file → no diagnostics, return
3. unresolved imports (via `OverlayResolver`)
4. import cycle (via `PackageIndex::import_cycle`)
5. full link — `wesl::compile_sourcemap`
6. type check — `PackageIndex::type_diagnostics`

The early exits are deliberate: a broken import would otherwise produce an avalanche of
downstream type errors. A new diagnostic kind needs a considered position in this order, not an
append.

`diagnostic_batch` walks the import closure so a fix in a dependency clears stale squiggles in
its dependents. It feeds both delivery mechanisms — see the capability surface below — and the
push path debounces 150 ms after `didChange`.

### OverlayResolver (`overlay.rs`)

Wraps `wesl::FileResolver` so unsaved editor buffers shadow the on-disk file. This is why imports
resolve against edits the user hasn't saved. Any path that resolves modules must go through it,
never `FileResolver` directly.

### Root discovery (`root.rs`)

A `wesl.toml` marker wins outright. Otherwise walk upward through *contiguous* directories that
contain shaders and stop at the first that doesn't. An explicit root from
`initializationOptions.root` or `workspace/configuration` overrides both.

### PackageIndex (`index.rs`)

Per-root index of every shader file, powering definition, references, rename, symbols, hover, and
completions. Types flow across module boundaries through `type_environment` /
`imported_type_environment`, which recursively analyze imported files and thread the resulting
`TypeEnvironment` in — `bind_alias` handles `import ... as Name` renames. The `active` set breaks
recursion on cyclic imports.

Note that this file does a lot of *textual* scanning — `brace_scopes`, `identifier_ranges`,
`declared_type_name`, `member_base_source`, `dot_before_identifier`. That is not laziness: these
paths must work against a possibly-stale AST while the buffer is syntactically broken, which is
exactly when completion is requested (`v.` doesn't parse). Prefer the AST when the tree is
trustworthy, text when it isn't.

### Type checker (`ty/mod.rs`)

`Ty` plus a `Checker` over the AST. `check_module` is the pure, import-free entry point used by
the differential tests; `analyze_module` is the real one, taking an inbound `TypeEnvironment` and
returning the outbound one.

### Formatter (`wesl-fmt`)

`format()` is a **gate that refuses to act when unsure** — it returns `None` (and logs why)
unless the output reparses, the AST round-trips identically (`syntax.to_string() ==
reparsed.to_string()`), and a second pass is a fixed point. Returning `None` means the editor
leaves the file untouched, which is always the right failure mode.

Mechanically it prints the AST, applies textual fixups (`reindent`, trailing commas, wrapping
long argument lists), then reattaches comments by aligning token streams between the original and
the printed output — the AST does not carry comments. `attach_comments` failing to align is a
legitimate reason to refuse.

## Capability surface

`capabilities()` in `crates/wesl-lsp/src/main.rs` advertises: incremental sync, definition,
references, rename (with prepare), document symbols, document highlight, workspace symbols,
folding ranges, selection ranges, hover, completion (trigger character `.`), signature help
(trigger `(`, retrigger `,`), inlay hints, whole-document formatting, and `willRenameFiles`
for `.wesl`/`.wgsl`.

`willRenameFiles` rewrites import paths that pointed at the renamed shader, for both file and
directory renames. It resolves each candidate through `module_file` before touching any text,
so a module of the same name in another package is never edited, and the path match requires a
segment boundary so `package::mesh` does not match inside `package::mesh_utils`.

A directory rename moves every shader beneath it, so `file_rename_edits` builds one rewrite
per moved shader. Note it skips nothing by path: a shader *inside* the renamed directory that
imports a sibling in that same directory needs rewriting too, which a `path == old_path` guard
would silently miss.

### Configuration

Settings live under the `wesl-lsp` section and are also accepted verbatim as
`initializationOptions`. Every field is optional; a partial object leaves the rest at its
default.

| Setting | Default |
|---|---|
| `root` | discovered |
| `inlayHints.enabled` | `true` |
| `inlayHints.typeHints` | `true` |
| `inlayHints.parameterHints` | `true` |
| `inlayHints.structLayoutHints` | **`false`** |

Layout hints default off deliberately: they annotate every member of every struct with
roughly 28 characters of virtual text regardless of what the reader is doing, and the
information is only actionable while reconciling a shader struct against a host-side one.

**Gate hint kinds in `wesl-analysis`, not in the protocol layer.** `InlayHintConfig` is
threaded down to `PackageIndex::inlay_hints`, which skips the work rather than filtering
results — layout and type hints each run a type-checker pass, so computing a disabled kind
and discarding it would cost a full pass per keystroke.

Two request shapes exist for settings and they are not interchangeable.
`request_configuration_blocking` is startup-only: it blocks for the response and queues
anything arriving first into `startup_messages`. Runtime refreshes triggered by
`didChangeConfiguration` go through `Server::request_configuration`, which sends the request
and picks the answer up in the normal loop via `pending_configuration`. Blocking at runtime
would deadlock against a client that is waiting on us.

Inlay hints come from three sources. Type hints reuse the type checker:
`inferred_declarations` runs the same pass as `analyze_module` but keeps the type it settled
on for each declaration that had no written annotation, so hints can never disagree with
diagnostics. Parameter hints are token-based, and are suppressed when the argument already
spells the parameter name. Layout hints come from `layout.rs`.

### Memory layout (`layout.rs`)

Implements the WGSL specification's alignment and size rules so struct members can be
annotated with their byte offset, alignment and size. This is the one hint that reports
something genuinely invisible in the source: `radius: f32` before `position: vec3<f32>` pushes
the vector to offset 16 and wastes twelve bytes, and a host-side struct that does not match
fails silently at runtime rather than erroring.

Two things keep it honest. Non-host-shareable types (`bool`, samplers, textures, pointers) and
runtime-sized arrays return `None` rather than a guess, and the whole struct is then skipped —
a wrong offset is worse than no offset. And `@align`/`@size` attributes are threaded through
by struct name, so a nested struct is measured with its own attributes rather than its natural
layout. The unit tests check the full matrix table from the specification directly.

`folding.rs`, `selection.rs` and `signature.rs` work from tokens and delimiter nesting rather
than the AST, so all three survive a buffer that does not parse — which is exactly the state
a buffer is in while a call is half-typed.
Folding returns byte ranges covering the whole construct and the protocol layer decides which
lines stay visible (a brace region keeps its closing `}`, a comment or import run does not).
Selection stops at delimiter granularity: expanding inside `a + b * c` jumps from the
identifier to the enclosing bracket rather than stepping through sub-expressions.

`workspace/symbol` searches the packages owning currently open documents, indexing each open
buffer's root on demand. Files that were never opened are still found, because `PackageIndex`
walks the whole root — but a root nobody has opened is not searched.

`didChange` applies each content change in order, resolving every ranged change against the
text as it stands just before that change. `save` is registered with `includeText`, so a save
is an authoritative full resync — that is what bounds the damage if a change ever fails to
apply, and why the failure path logs and continues rather than trying to guess.

Diagnostics use **exactly one mechanism per session**, negotiated at initialize. A client that
declares pull support gets `textDocument/diagnostic` and no pushes; everyone else keeps the
push path, which is what Zed needs. Advertising both would double-report in clients that do
both, and the specification advises against mixing them. The pull report carries the rest of
the import closure in `relatedDocuments`, preserving the push path's behaviour of clearing
stale squiggles in dependents.

Deliberately absent, with the reasoning:

- **Position encoding negotiation** — UTF-16 is assumed and correct; `line_index.rs` converts
  properly and has a test covering astral-plane characters. Negotiating UTF-8 would save
  conversion work, but nothing is broken today.

Not yet implemented, ordered roughly by value to shader authors:

- **Range formatting** — `wesl-fmt::format` is whole-document by construction; range support
  needs a way to bound the AST print, which is not a small change.
- **Semantic tokens and code actions** — no work started.

Adding a capability means touching four layers in order: `capabilities()`, a `METHOD` arm in
`handle_request`, an offset-based method on `AnalysisHost`, and a case in `lsp_smoke.rs` — which
is the only test that exercises protocol wiring.

## Testing philosophy

The invariant this project defends is **no false diagnostics on valid shaders**. Users abandon a
language server that lights up correct code.

- `differential.rs` uses **naga as an oracle** and asserts *bidirectional* agreement over the
  corpus: the checker errors if and only if naga errors. Deviating in either direction fails.
- `NAGA_LAG_EXCEPTIONS` and `NON_TYPE_OR_COMPOSED_EXCEPTIONS` are how known divergence is
  recorded. Adding an entry is an admission of a real gap — do it consciously, with a reason, not
  to turn a test green.
- `public_corpus.rs` / `private_corpus.rs` are pure false-positive gates: valid shaders, zero
  diagnostics.
- `span_fidelity.rs` asserts every span the parser reports slices to the exact expected source
  text. Span drift is what turns a correct diagnostic into a squiggle under the wrong token.
- `lsp_smoke.rs` spawns the real binary and speaks JSON-RPC over stdio — the only test that
  covers protocol wiring.

## Conventions

- **Identifiers are spelled out in full.** `arguments`, `declaration`, `extension`, `overload`,
  `entry` — never `args`, `decl`, `ext`. This holds throughout; match it.
- Edition 2024 idioms are used freely: let-chains (`if let Some(x) = a && let Some(y) = b`),
  `let ... else` for early exit, `.filter_map(Result::ok)` over `WalkDir`.
- `crates/wesl-analysis/src/builtins.rs` is **generated** and carries a header saying so. Edit
  `generate_builtins` in `xtask/src/main.rs` and regenerate; hand edits are lost.
- Corpora are pinned by commit SHA in `xtask/src/main.rs` and fetched by sparse shallow checkout.
  Changing a pin changes the meaning of every corpus test — expect to re-audit the exception lists.

## Release

Pushing a `v*` tag triggers `.github/workflows/release.yml`, which cross-builds `wesl-lsp` for
macOS (arm64/x86_64), Linux (arm64/x86_64), and Windows x86_64. A separate `release` job waits on
all five targets and publishes them in one step, so a partial matrix never yields a partial
release. Version lives in `[workspace.package]` in the root `Cargo.toml`.
