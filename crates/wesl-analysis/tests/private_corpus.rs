use std::{env, fs, path::PathBuf};

use wesl_analysis::AnalysisHost;

#[test]
fn private_corpus_has_no_false_diagnostics() {
    let Some(root) = env::var_os("WESL_LSP_PRIVATE_CORPUS").map(PathBuf::from) else {
        return;
    };
    let mut failures = Vec::new();
    for entry in fs::read_dir(&root).unwrap().filter_map(Result::ok) {
        let path = entry.path();
        if !matches!(
            path.extension().and_then(|extension| extension.to_str()),
            Some("wesl" | "wgsl")
        ) {
            continue;
        }
        let source = fs::read_to_string(&path).unwrap();
        let mut analysis = AnalysisHost::new(Some(root.clone()));
        analysis.open(path.clone(), source);
        let diagnostics = analysis.diagnostics(&path);
        if !diagnostics.is_empty() {
            failures.push((path, diagnostics));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}
