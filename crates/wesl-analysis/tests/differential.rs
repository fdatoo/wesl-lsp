use std::{
    env, fs,
    path::{Path, PathBuf},
};

use wesl::{CompileOptions, EscapeMangler, StandardResolver};
use wesl_analysis::check_module;
use wgsl_parse::{
    parse_str,
    syntax::{ModulePath, PathOrigin},
};

const NAGA_LAG_EXCEPTIONS: &[&str] = &[];

#[test]
fn private_corpus_checker_clean_implies_naga_clean() {
    let Some(root) = env::var_os("WESL_LSP_PRIVATE_CORPUS").map(PathBuf::from) else {
        return;
    };
    for entry in fs::read_dir(&root).unwrap().filter_map(Result::ok) {
        let path = entry.path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("wesl") {
            continue;
        }
        check_clean_file(&root, &path);
    }
}

#[test]
fn checker_error_implies_naga_error() {
    let cases = [("vector_to_scalar", "fn main() { let x: f32 = vec3(1.0); }")];
    for (name, source) in cases {
        let module = parse_str(source).unwrap();
        assert!(!check_module(&module).is_empty(), "checker missed {name}");
        if !NAGA_LAG_EXCEPTIONS.contains(&name) {
            assert!(
                naga_validate(source).is_err(),
                "checker false positive for {name}"
            );
        }
    }
}

fn check_clean_file(root: &Path, path: &Path) {
    let source = fs::read_to_string(path).unwrap();
    let module = parse_str(&source).unwrap();
    let checker_diagnostics = check_module(&module);
    if !checker_diagnostics.is_empty() {
        return;
    }

    let module_name = path
        .strip_prefix(root)
        .unwrap()
        .with_extension("")
        .components()
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .collect();
    let root_module = ModulePath::new(PathOrigin::Absolute, module_name);
    let resolver = StandardResolver::new(root);
    let options = CompileOptions {
        strip: false,
        lazy: false,
        ..CompileOptions::default()
    };
    let compiled = wesl::compile_sourcemap(&root_module, &resolver, &EscapeMangler, &options)
        .unwrap_or_else(|error| panic!("wesl failed for {}: {error}", path.display()));
    let wgsl = compiled.syntax.to_string();
    naga_validate(&wgsl)
        .unwrap_or_else(|error| panic!("naga failed for {}: {error}\n{wgsl}", path.display()));
}

fn naga_validate(source: &str) -> Result<(), String> {
    let module = naga::front::wgsl::parse_str(source).map_err(|error| error.to_string())?;
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .map(|_| ())
    .map_err(|error| error.to_string())
}
