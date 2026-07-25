use std::{fs, path::PathBuf};

use walkdir::WalkDir;

/// With `WESL_LSP_REQUIRE_CORPORA` set, a missing corpus fails instead of skipping.
///
/// Every corpus gate returns early when its input is absent, which means a half-successful
/// `fetch-corpus` turns the whole suite green while proving nothing. CI sets this so that
/// silence becomes a failure.
fn require_corpora() -> bool {
    std::env::var_os("WESL_LSP_REQUIRE_CORPORA").is_some()
}

#[test]
fn public_corpus_formatting_is_idempotent() {
    let _ = env_logger::builder().is_test(true).try_init();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../corpus");
    if !root.exists() {
        assert!(
            !require_corpora(),
            "corpus missing at {}; run `cargo run -p xtask -- fetch-corpus`",
            root.display()
        );
        return;
    }
    let mut formatted_count = 0;
    for entry in WalkDir::new(root).into_iter().filter_map(Result::ok) {
        let path = entry.path();
        if !entry.file_type().is_file()
            || path.extension().and_then(|extension| extension.to_str()) != Some("wgsl")
        {
            continue;
        }
        let source = fs::read_to_string(path).unwrap();
        if wgsl_parse::parse_str(&source).is_err() {
            continue;
        }
        let formatted = wesl_fmt::format(&source, 4, path)
            .unwrap_or_else(|| panic!("formatter gate rejected {}", path.display()));
        let second = wesl_fmt::format(&formatted, 4, path)
            .unwrap_or_else(|| panic!("formatter rejected its own output for {}", path.display()));
        assert_eq!(formatted, second, "{}", path.display());
        formatted_count += 1;
    }
    assert!(
        formatted_count >= 100,
        "only formatted {formatted_count} public shaders"
    );
}

#[test]
fn private_corpus_formatting_is_idempotent() {
    let Some(root) = std::env::var_os("WESL_LSP_PRIVATE_CORPUS").map(PathBuf::from) else {
        return;
    };
    let mut formatted_count = 0;
    for entry in WalkDir::new(root).into_iter().filter_map(Result::ok) {
        let path = entry.path();
        if !entry.file_type().is_file()
            || path.extension().and_then(|extension| extension.to_str()) != Some("wesl")
        {
            continue;
        }
        let source = fs::read_to_string(path).unwrap();
        let formatted = wesl_fmt::format(&source, 4, path)
            .unwrap_or_else(|| panic!("formatter gate rejected {}", path.display()));
        let second = wesl_fmt::format(&formatted, 4, path)
            .unwrap_or_else(|| panic!("formatter rejected its own output for {}", path.display()));
        assert_eq!(formatted, second, "{}", path.display());
        formatted_count += 1;
    }
    assert!(
        formatted_count >= 20,
        "only formatted {formatted_count} private shaders"
    );
}
