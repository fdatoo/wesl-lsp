use std::{
    env, fs,
    path::{Path, PathBuf},
};

use walkdir::WalkDir;
use wesl::{CompileOptions, EscapeMangler, StandardResolver};
use wesl_analysis::check_module;
use wgsl_parse::{
    parse_str,
    syntax::{ModulePath, PathOrigin},
};

const NAGA_LAG_EXCEPTIONS: &[&str] = &[
    "wesl-rs/crates/wesl-test/wgpu/out/glsl-expressions.frag.wgsl",
    "wesl-rs/crates/wesl-test/wgpu/out/glsl-samplers.frag.wgsl",
    "wesl-rs/crates/wesl-test/wgpu/out/glsl-images.frag.wgsl",
    "wesl-rs/crates/wesl-test/wgpu/in/overrides-ray-query.wgsl",
    "wesl-rs/crates/wesl-test/wgpu/in/ray-query.wgsl",
];

const NON_TYPE_OR_COMPOSED_EXCEPTIONS: &[&str] = &[
    "wesl-rs/crates/wesl-test/wgpu/in/debug-symbol-large-source.wgsl",
    "wesl-rs/crates/wesl-test/wgpu/in/debug-symbol-terrain.wgsl",
    "webgpu-samples/sample/cornell/radiosity.wgsl",
    "webgpu-samples/sample/cornell/rasterizer.wgsl",
    "webgpu-samples/sample/cornell/raytracer.wgsl",
    "webgpu-samples/sample/skinnedMesh/gltf.wgsl",
];

#[test]
fn public_corpus_matches_naga_bidirectionally() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../corpus");
    if !root.exists() {
        return;
    }
    let mut compared = 0;
    let mut checker_flagged = 0;
    let mut naga_flagged = 0;
    let mut mismatches = Vec::new();
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
            line.starts_with('#') || line.starts_with("import ")
        }) {
            continue;
        }
        let Ok(module) = parse_str(&source) else {
            continue;
        };
        compared += 1;
        let checker_diagnostics = check_module(&module);
        let checker_error = !checker_diagnostics.is_empty();
        checker_flagged += usize::from(checker_error);
        let naga_result = naga_validate(&source);
        let naga_error = naga_result.is_err();
        naga_flagged += usize::from(naga_error);
        let relative = path.strip_prefix(&root).unwrap().to_string_lossy();
        if checker_error != naga_error
            && !NAGA_LAG_EXCEPTIONS.contains(&relative.as_ref())
            && !NON_TYPE_OR_COMPOSED_EXCEPTIONS.contains(&relative.as_ref())
        {
            mismatches.push((
                path.to_path_buf(),
                checker_error,
                checker_diagnostics,
                naga_result.err().unwrap_or_default(),
            ));
        }
    }
    assert!(compared >= 100, "only compared {compared} public shaders");
    assert!(mismatches.is_empty(), "{mismatches:#?}");
    assert!(
        checker_flagged > 0 && naga_flagged > 0,
        "oracle did not exercise both error paths: checker={checker_flagged}, naga={naga_flagged}"
    );
}

#[test]
fn private_corpus_matches_naga_bidirectionally() {
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
    let checker_error = !check_module(&compiled.syntax).is_empty();
    let naga_error = naga_validate(&wgsl).is_err();
    assert_eq!(
        checker_error,
        naga_error,
        "checker/naga disagreement for {}\n{wgsl}",
        path.display()
    );
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
