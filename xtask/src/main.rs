use std::{
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, bail};
use serde_json::Value;

fn main() -> Result<()> {
    let mut arguments = env::args().skip(1);
    match arguments.next().as_deref() {
        Some("generate-builtins") => {
            let source = arguments
                .next()
                .map(PathBuf::from)
                .context("usage: xtask generate-builtins <functions.json>")?;
            generate_builtins(&source)
        }
        Some("fetch-corpus") => fetch_corpus(),
        Some(command) => bail!("unknown xtask command: {command}"),
        None => bail!("usage: xtask <generate-builtins|fetch-corpus>"),
    }
}

fn generate_builtins(source: &PathBuf) -> Result<()> {
    let functions: BTreeMap<String, Value> = serde_json::from_str(&fs::read_to_string(source)?)?;
    let mut output = String::from(
        "// Generated from nolanderc/wgsl-spec 0.2.0 (MIT), commit 722a608ca9119a9e83558c5b63eca61542717f4e.\n\
         // Regenerate with `cargo run -p xtask -- generate-builtins <functions.json>`.\n\n\
         #[derive(Clone, Copy, Debug)]\n\
         pub struct BuiltinOverload {\n    pub signature: &'static str,\n    pub doc: &'static str,\n}\n\n\
         #[derive(Clone, Copy, Debug)]\n\
         pub struct BuiltinFn {\n    pub name: &'static str,\n    pub overloads: &'static [BuiltinOverload],\n}\n\n\
         pub static BUILTIN_FUNCTIONS: &[BuiltinFn] = &[\n",
    );
    for (name, function) in functions {
        output.push_str(&format!(
            "    BuiltinFn {{ name: {}, overloads: &[\n",
            raw_string(&name)
        ));
        let function_doc = function["description"].as_str().unwrap_or_default();
        for overload in function["overloads"].as_array().into_iter().flatten() {
            let signature = overload["signature"].as_str().unwrap_or_default();
            let doc = overload["description"].as_str().unwrap_or(function_doc);
            output.push_str(&format!(
                "        BuiltinOverload {{ signature: {}, doc: {} }},\n",
                raw_string(signature),
                raw_string(doc)
            ));
        }
        output.push_str("    ] },\n");
    }
    output.push_str(
        "];\n\n\
         pub fn builtin(name: &str) -> Option<&'static BuiltinFn> {\n\
             BUILTIN_FUNCTIONS.binary_search_by_key(&name, |builtin| builtin.name).ok().map(|index| &BUILTIN_FUNCTIONS[index])\n\
         }\n\n\
         pub static BUILTIN_TYPES: &[&str] = &[\n\
             \"array\", \"atomic\", \"bool\", \"f16\", \"f32\", \"i32\", \"mat2x2\", \"mat2x3\", \"mat2x4\",\n\
             \"mat3x2\", \"mat3x3\", \"mat3x4\", \"mat4x2\", \"mat4x3\", \"mat4x4\", \"ptr\", \"sampler\",\n\
             \"sampler_comparison\", \"texture_1d\", \"texture_2d\", \"texture_2d_array\", \"texture_3d\",\n\
             \"texture_cube\", \"texture_cube_array\", \"texture_depth_2d\", \"texture_depth_2d_array\",\n\
             \"texture_depth_cube\", \"texture_depth_cube_array\", \"texture_external\", \"texture_multisampled_2d\",\n\
             \"texture_storage_1d\", \"texture_storage_2d\", \"texture_storage_2d_array\", \"texture_storage_3d\",\n\
             \"u32\", \"vec2\", \"vec3\", \"vec4\",\n\
         ];\n",
    );
    let destination =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../crates/wesl-analysis/src/builtins.rs");
    fs::write(destination, output)?;
    Ok(())
}

fn fetch_corpus() -> Result<()> {
    const CORPORA: &[(&str, &str, &str, &str)] = &[
        (
            "wesl-rs",
            "https://github.com/wgsl-tooling-wg/wesl-rs.git",
            "22a9b77dd1c099fb7dfeddec0977f68bc57b17c2",
            "crates/wesl-test",
        ),
        (
            "wgpu",
            "https://github.com/gfx-rs/wgpu.git",
            "d5977df452de52ffa0051f4cb3fb0de8fb59a059",
            "examples",
        ),
        (
            "webgpu-samples",
            "https://github.com/webgpu/webgpu-samples.git",
            "4181da1b8d4e3d4fe5ea52fc1150fe5200b87515",
            "sample",
        ),
        (
            "bevy",
            "https://github.com/bevyengine/bevy.git",
            "852cca833a5a8c3a7f553fcbac006b188f858ec4",
            "crates/bevy_pbr/src/render",
        ),
    ];
    let corpus_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../corpus");
    fs::create_dir_all(&corpus_root)?;
    for (name, repository, revision, sparse_path) in CORPORA {
        let destination = corpus_root.join(name);
        let pin = destination.join(".wesl-lsp-revision");
        if fs::read_to_string(&pin).ok().as_deref() == Some(*revision) {
            continue;
        }
        if destination.exists() {
            fs::remove_dir_all(&destination)?;
        }
        fs::create_dir_all(&destination)?;
        git(&destination, ["init", "--quiet"])?;
        git(&destination, ["remote", "add", "origin", repository])?;
        git(&destination, ["sparse-checkout", "init", "--cone"])?;
        git(&destination, ["sparse-checkout", "set", sparse_path])?;
        git(&destination, ["fetch", "--depth", "1", "origin", revision])?;
        git(
            &destination,
            ["checkout", "--quiet", "--detach", "FETCH_HEAD"],
        )?;
        fs::write(pin, revision)?;
    }
    Ok(())
}

fn git<const N: usize>(directory: &Path, arguments: [&str; N]) -> Result<()> {
    let status = Command::new("git")
        .args(arguments)
        .current_dir(directory)
        .status()
        .with_context(|| format!("running git in {}", directory.display()))?;
    if !status.success() {
        bail!("git failed in {}", directory.display());
    }
    Ok(())
}

fn raw_string(value: &str) -> String {
    let mut hashes = String::from("#");
    while value.contains(&format!("\"{hashes}")) {
        hashes.push('#');
    }
    format!("r{hashes}\"{value}\"{hashes}")
}
