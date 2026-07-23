use std::{fs, path::PathBuf};

use walkdir::WalkDir;
use wesl_analysis::check_module;

#[test]
fn checker_has_no_false_errors_on_public_wgsl() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../corpus");
    if !root.exists() {
        return;
    }
    let mut parsed = 0;
    let mut oracle_valid = 0;
    let mut dialect = 0;
    let mut failures = Vec::new();
    for entry in WalkDir::new(&root).into_iter().filter_map(Result::ok) {
        let path = entry.path();
        if !entry.file_type().is_file()
            || path.extension().and_then(|extension| extension.to_str()) != Some("wgsl")
        {
            continue;
        }
        let source = fs::read_to_string(path).unwrap();
        if source.lines().any(|line| {
            let line = line.trim_start();
            line.starts_with("#import") || line.starts_with("#define_import_path")
        }) {
            dialect += 1;
            continue;
        }
        let Ok(module) = wgsl_parse::parse_str(&source) else {
            continue;
        };
        parsed += 1;
        let Ok(naga_module) = naga::front::wgsl::parse_str(&source) else {
            continue;
        };
        if naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&naga_module)
        .is_err()
        {
            continue;
        }
        oracle_valid += 1;
        let diagnostics = check_module(&module);
        if !diagnostics.is_empty() {
            failures.push((path.to_path_buf(), diagnostics));
        }
    }
    assert!(parsed >= 100, "only parsed {parsed} public shaders");
    assert!(
        oracle_valid >= 75,
        "only validated {oracle_valid} public shaders"
    );
    assert!(dialect >= 20, "only found {dialect} naga_oil shaders");
    assert!(failures.is_empty(), "{failures:#?}");
}
