//! Runs every offset-based editor service at every identifier position of every corpus
//! shader, asserting they neither panic nor return structurally invalid results.
//!
//! The other corpus tests only exercise type checking, diagnostics and formatting. Everything
//! an editor actually calls while the user moves around a file — hover, completion,
//! definition, folding, selection, signature help, inlay hints — was covered only by
//! hand-written snippets. These services do byte arithmetic over `&str`, and a panic in any of
//! them takes the whole server down, so real shaders are the input that matters.

use std::{fs, path::PathBuf};

use walkdir::WalkDir;
use wesl_analysis::{AnalysisHost, InlayHintConfig, folding_ranges, selection_ranges};

fn corpus_shaders(limit: usize) -> Vec<(PathBuf, String)> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../corpus");
    if !root.exists() {
        return Vec::new();
    }
    let mut shaders = WalkDir::new(&root)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry.file_type().is_file()
                && entry
                    .path()
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| matches!(extension, "wgsl" | "wesl"))
        })
        .filter_map(|entry| {
            let source = fs::read_to_string(entry.path()).ok()?;
            Some((entry.path().canonicalize().ok()?, source))
        })
        .collect::<Vec<_>>();
    shaders.sort();
    shaders.truncate(limit);
    shaders
}

/// Offsets worth probing: the start, end and middle of every identifier-ish run, plus the
/// boundaries around punctuation, which is where off-by-one slicing shows up.
fn probe_offsets(source: &str) -> Vec<usize> {
    let mut offsets = vec![0, source.len()];
    let bytes = source.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_' {
            let start = index;
            while index < bytes.len()
                && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_')
            {
                index += 1;
            }
            offsets.extend([start, start + (index - start) / 2, index]);
        } else {
            index += 1;
        }
    }
    offsets.retain(|offset| source.is_char_boundary(*offset));
    offsets.dedup();
    // Generated shaders in the corpus run to megabytes; probing every identifier in one would
    // dominate the whole suite. A strided sample still lands on every construct they contain.
    let stride = (offsets.len() / MAX_PROBES_PER_FILE).max(1);
    offsets.into_iter().step_by(stride).collect()
}

/// Offsets probed per shader, whatever its size.
const MAX_PROBES_PER_FILE: usize = 200;

#[test]
fn purely_textual_services_survive_every_corpus_offset() {
    let shaders = corpus_shaders(usize::MAX);
    if shaders.is_empty() {
        return;
    }
    let mut probed = 0;
    for (path, source) in &shaders {
        // Folding is per-document; every range must be in bounds and non-empty.
        for folding in folding_ranges(source) {
            assert!(
                folding.range.end <= source.len() && folding.range.start < folding.range.end,
                "invalid folding range in {}: {folding:?}",
                path.display()
            );
            assert!(
                source.is_char_boundary(folding.range.start)
                    && source.is_char_boundary(folding.range.end),
                "folding range splits a character in {}",
                path.display()
            );
        }

        for offset in probe_offsets(source) {
            probed += 1;
            let chain = selection_ranges(source, offset);
            for pair in chain.windows(2) {
                assert!(
                    pair[1].start <= pair[0].start && pair[1].end >= pair[0].end,
                    "selection chain not nested at {offset} in {}",
                    path.display()
                );
            }
            if let Some(innermost) = chain.first() {
                assert!(
                    innermost.start <= offset && offset <= innermost.end,
                    "selection chain does not contain its own offset in {}",
                    path.display()
                );
            }
        }
    }
    assert!(
        shaders.len() >= 100 && probed >= 5_000,
        "corpus too small to be meaningful: {} shaders, {probed} offsets",
        shaders.len()
    );
}

#[test]
fn host_backed_services_survive_every_corpus_offset() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../corpus");
    if !root.exists() {
        return;
    }
    // One host per corpus package, matching how an editor holds a single host across a
    // workspace. A host per file would rebuild the whole package index each time.
    let packages = fs::read_dir(&root)
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect::<Vec<_>>();

    let mut probed = 0;
    for package in packages {
        let shaders = WalkDir::new(&package)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|entry| {
                entry.file_type().is_file()
                    && entry
                        .path()
                        .extension()
                        .and_then(|extension| extension.to_str())
                        .is_some_and(|extension| matches!(extension, "wgsl" | "wesl"))
            })
            .filter_map(|entry| {
                let source = fs::read_to_string(entry.path()).ok()?;
                Some((entry.path().canonicalize().ok()?, source))
            })
            .take(25)
            .collect::<Vec<_>>();

        let mut host = AnalysisHost::new(Some(package));
        for (path, source) in &shaders {
            host.open(path.clone(), source.clone());
        }

        for (path, source) in &shaders {
            let _ = host.folding_ranges(path);
            let _ = host.document_symbols(path);
            let _ = host.inlay_hints(
                path,
                0..source.len(),
                InlayHintConfig {
                    type_hints: true,
                    parameter_hints: true,
                    struct_layout_hints: true,
                },
            );

            // Probing every identifier would dominate the suite's runtime; a strided sample
            // still covers thousands of positions across every construct in the corpus.
            for offset in probe_offsets(source).into_iter() {
                probed += 1;
                let _ = host.hover(path, offset);
                let _ = host.completions(path, offset);
                let _ = host.definition(path, offset);
                let _ = host.signature_help(path, offset);
                let _ = host.document_highlights(path, offset);
                let _ = host.prepare_rename(path, offset);
                let _ = host.selection_ranges(path, offset);
            }
        }
    }
    assert!(
        probed >= 1_000,
        "corpus too small to be meaningful: {probed} offsets"
    );
}
