//! Runs every editor service over deliberately damaged shaders.
//!
//! `editor_services_corpus.rs` covers the corpus as written — that is, valid shaders. But a
//! buffer is *broken* for most of the time anyone is typing in it, which is exactly when
//! completion, signature help and folding get asked for. These services do byte arithmetic on
//! `&str` and a panic in any of them takes the whole server down, so damaged input is the case
//! that matters and the one hardest to reach by hand.
//!
//! Mutations are seeded deterministically: a failure reports the shader and seed that produced
//! it, and rerunning reproduces exactly.

use std::{fs, path::PathBuf};

use walkdir::WalkDir;
use wesl_analysis::{
    AnalysisHost, InlayHintConfig, PositionEncoding, folding_ranges, reindent_line,
    selection_ranges,
};

#[path = "common/mod.rs"]
mod common;

/// xorshift64*, so the corpus sweep is reproducible without a dependency.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, bound: usize) -> usize {
        if bound == 0 {
            0
        } else {
            (self.next() % bound as u64) as usize
        }
    }
}

/// Damage that mirrors what a half-finished edit looks like: truncation, unbalanced
/// delimiters, deleted spans, and junk spliced into the middle of a token.
fn mutate(source: &str, rng: &mut Rng) -> String {
    const JUNK: &[&str] = &[
        "{", "}", "(", ")", "<", ">", ";", ",", "\"", "/*", "*/", "//", "fn", "@", "->", "😀",
        "\\", "#ifdef", "\u{0}",
    ];
    let mut text = source.to_owned();
    for _ in 0..rng.below(4) + 1 {
        if text.is_empty() {
            break;
        }
        let mut at = rng.below(text.len());
        while !text.is_char_boundary(at) {
            at -= 1;
        }
        match rng.below(5) {
            // Truncate, which is what an unfinished paste or a partial read looks like.
            0 => text.truncate(at),
            // Delete a span, possibly cutting a construct in half.
            1 => {
                let mut end = at + rng.below(text.len() - at + 1);
                while !text.is_char_boundary(end) {
                    end -= 1;
                }
                text.replace_range(at..end, "");
            }
            // Splice junk in, unbalancing delimiters or opening a comment that never closes.
            2 => text.insert_str(at, JUNK[rng.below(JUNK.len())]),
            // Duplicate a slice, producing repeated declarations.
            3 => {
                let mut end = (at + rng.below(200)).min(text.len());
                while !text.is_char_boundary(end) {
                    end -= 1;
                }
                let slice = text[at..end].to_owned();
                text.insert_str(at, &slice);
            }
            // Overwrite a byte range with junk, mangling a token from the inside.
            _ => {
                let mut end = (at + rng.below(8)).min(text.len());
                while !text.is_char_boundary(end) {
                    end -= 1;
                }
                text.replace_range(at..end, JUNK[rng.below(JUNK.len())]);
            }
        }
    }
    text
}

fn probe_offsets(source: &str, limit: usize) -> Vec<usize> {
    if source.is_empty() {
        return vec![0];
    }
    let stride = (source.len() / limit).max(1);
    let mut offsets = (0..=source.len())
        .step_by(stride)
        .filter(|offset| source.is_char_boundary(*offset))
        .collect::<Vec<_>>();
    // The very end is where off-by-one slicing shows up.
    offsets.push(source.len());
    offsets.dedup();
    offsets
}

fn seed_shaders(limit: usize) -> Vec<(PathBuf, String)> {
    // Callers already gate on `common::corpus_root().exists()` before calling this.
    let root = common::corpus_root();
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
            // Huge generated shaders make every mutation round slow without adding variety.
            (source.len() < 40_000).then(|| (entry.path().to_path_buf(), source))
        })
        .collect::<Vec<_>>();
    shaders.sort();
    shaders.truncate(limit);
    shaders
}

#[test]
fn textual_services_survive_damaged_shaders() {
    let root = common::corpus_root();
    if !root.exists() {
        assert!(
            !common::require_corpora(),
            "corpus missing at {}; run `cargo run -p xtask -- fetch-corpus`",
            root.display()
        );
        return;
    }
    let shaders = seed_shaders(60);
    let mut rounds = 0;
    for (path, source) in &shaders {
        for seed in 1..12u64 {
            let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
            let damaged = mutate(source, &mut rng);
            rounds += 1;

            for folding in folding_ranges(&damaged) {
                assert!(
                    folding.range.end <= damaged.len()
                        && damaged.is_char_boundary(folding.range.start)
                        && damaged.is_char_boundary(folding.range.end),
                    "{}: seed {seed} produced an invalid folding range {folding:?}",
                    path.display()
                );
            }

            for offset in probe_offsets(&damaged, 60) {
                let chain = selection_ranges(&damaged, offset);
                for pair in chain.windows(2) {
                    assert!(
                        pair[1].start <= pair[0].start && pair[1].end >= pair[0].end,
                        "{}: seed {seed} broke selection nesting at {offset}",
                        path.display()
                    );
                }
                if let Some((range, indent)) = reindent_line(&damaged, offset, 4) {
                    assert!(
                        range.end <= damaged.len() && indent.chars().all(|c| c == ' '),
                        "{}: seed {seed} produced a bad indent edit at {offset}",
                        path.display()
                    );
                }
            }
        }
    }
    assert!(rounds >= 500, "only ran {rounds} mutation rounds");
}

#[test]
fn host_services_survive_damaged_shaders() {
    let root = common::corpus_root();
    if !root.exists() {
        assert!(
            !common::require_corpora(),
            "corpus missing at {}; run `cargo run -p xtask -- fetch-corpus`",
            root.display()
        );
        return;
    }
    let shaders = seed_shaders(25);
    let mut rounds = 0;
    for (path, source) in &shaders {
        for seed in 1..6u64 {
            let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
            let damaged = mutate(source, &mut rng);
            rounds += 1;

            // A fresh host per round: the point is a mangled buffer, not accumulated state.
            let mut host = AnalysisHost::default();
            host.open(path.clone(), damaged.clone());

            let _ = host.diagnostics(path);
            let _ = host.document_symbols(path);
            let _ = host.folding_ranges(path);
            let _ = host.inlay_hints(path, 0..damaged.len(), InlayHintConfig::default());

            for offset in probe_offsets(&damaged, 25) {
                let _ = host.hover(path, offset);
                let _ = host.completions(path, offset);
                let _ = host.definition(path, offset);
                let _ = host.signature_help(path, offset);
                let _ = host.prepare_rename(path, offset);
                let _ = host.document_highlights(path, offset);
                let _ = host.rename(path, offset, "renamed");
            }
        }
    }
    assert!(rounds >= 100, "only ran {rounds} mutation rounds");
}

/// Damaged text must still round-trip through both position encodings without panicking, since
/// a mangled buffer is exactly when a client sends a position the server did not expect.
#[test]
fn positions_stay_sane_on_damaged_shaders() {
    let root = common::corpus_root();
    if !root.exists() {
        assert!(
            !common::require_corpora(),
            "corpus missing at {}; run `cargo run -p xtask -- fetch-corpus`",
            root.display()
        );
        return;
    }
    let shaders = seed_shaders(30);
    let mut round_trips = 0;
    for (_, source) in &shaders {
        for seed in 1..8u64 {
            let mut rng = Rng(seed.wrapping_mul(0x2545_F491_4F6C_DD1D) | 1);
            let damaged = mutate(source, &mut rng);
            for encoding in [PositionEncoding::Utf16, PositionEncoding::Utf8] {
                let lines = wesl_analysis::LineIndex::new(&damaged, encoding);
                for offset in probe_offsets(&damaged, 40) {
                    let Some(position) = lines.offset_to_position(&damaged, offset) else {
                        continue;
                    };
                    assert_eq!(
                        lines.position_to_offset(&damaged, position),
                        Some(offset),
                        "seed {seed}, {encoding:?}: position round trip failed at {offset}"
                    );
                    round_trips += 1;
                }
            }
        }
    }
    assert!(
        round_trips >= 5_000,
        "only checked {round_trips} position round trips"
    );
}
