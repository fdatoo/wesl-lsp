//! Shared helpers for corpus-gated integration tests in this crate.

use std::path::PathBuf;

/// With `WESL_LSP_REQUIRE_CORPORA` set to a non-empty value other than `"0"`, a missing corpus
/// fails instead of skipping.
///
/// Every corpus gate returns early when its input is absent, which means a half-successful
/// `fetch-corpus` turns the whole suite green while proving nothing. CI sets this so that
/// silence becomes a failure. `var_os(..).is_some()` would treat an *empty* override as "set",
/// but CI's skip-mode step exports `WESL_LSP_REQUIRE_CORPORA: ""` expecting that to mean off
/// (GitHub Actions exports empty-valued env vars rather than unsetting them), so empty and
/// `"0"` both count as off here.
pub fn require_corpora() -> bool {
    std::env::var("WESL_LSP_REQUIRE_CORPORA").is_ok_and(|value| !value.is_empty() && value != "0")
}

/// The root directory `xtask fetch-corpus` populates, shared by every corpus-gated test so the
/// location is stated once instead of re-derived (and potentially drifting) per test file.
pub fn corpus_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../corpus")
}
