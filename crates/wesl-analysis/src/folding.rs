//! Foldable regions, computed from token and line structure rather than the AST so that
//! folding keeps working while the buffer is mid-edit and does not parse.

use std::ops::Range;

use crate::index::brace_scopes;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FoldKind {
    Region,
    Comment,
    Imports,
}

/// A foldable region as a byte range over the source. `range` covers the whole construct —
/// brace to brace, or the first through last line of a run. Deciding which of those lines
/// stay visible once folded is a presentation choice, so the protocol layer makes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FoldingRange {
    pub range: Range<usize>,
    pub kind: FoldKind,
}

pub fn folding_ranges(source: &str) -> Vec<FoldingRange> {
    let mut ranges = brace_scopes(source)
        .into_iter()
        .map(|range| FoldingRange {
            range,
            kind: FoldKind::Region,
        })
        .collect::<Vec<_>>();
    ranges.extend(line_runs(source, FoldKind::Imports, is_import_line));
    ranges.extend(line_runs(source, FoldKind::Comment, is_line_comment));
    ranges.extend(block_comments(source));
    // A region confined to one line hides nothing when collapsed.
    ranges.retain(|folding| source[folding.range.clone()].contains('\n'));
    ranges.sort_by(|left, right| {
        left.range
            .start
            .cmp(&right.range.start)
            .then(right.range.end.cmp(&left.range.end))
    });
    ranges.dedup();
    ranges
}

/// Groups consecutive lines satisfying `matches` into one region per run. Single-line runs
/// are dropped, since folding one line hides nothing.
fn line_runs(
    source: &str,
    kind: FoldKind,
    matches: fn(&str) -> bool,
) -> impl Iterator<Item = FoldingRange> {
    let mut runs = Vec::new();
    let mut current: Option<Range<usize>> = None;
    let mut offset = 0;
    for line in source.split_inclusive('\n') {
        let content_end = offset + line.trim_end_matches(['\n', '\r']).len();
        if matches(line) {
            current = Some(match current {
                Some(run) => run.start..content_end,
                None => offset..content_end,
            });
        } else if let Some(run) = current.take() {
            runs.push(run);
        }
        offset += line.len();
    }
    runs.extend(current);
    runs.into_iter()
        .map(move |range| FoldingRange { range, kind })
}

fn block_comments(source: &str) -> Vec<FoldingRange> {
    let bytes = source.as_bytes();
    let mut ranges = Vec::new();
    let mut index = 0;
    while index + 1 < bytes.len() {
        if bytes[index..].starts_with(b"//") {
            while index < bytes.len() && bytes[index] != b'\n' {
                index += 1;
            }
            continue;
        }
        if bytes[index..].starts_with(b"/*") {
            let start = index;
            index += 2;
            while index + 1 < bytes.len() && !bytes[index..].starts_with(b"*/") {
                index += 1;
            }
            let end = (index + 2).min(bytes.len());
            if source[start..end].contains('\n') {
                ranges.push(FoldingRange {
                    range: start..end,
                    kind: FoldKind::Comment,
                });
            }
            index = end;
            continue;
        }
        index += 1;
    }
    ranges
}

fn is_import_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed
        .strip_prefix("import")
        .or_else(|| trimmed.strip_prefix("#import"))
        .is_some_and(|rest| rest.starts_with(char::is_whitespace))
}

fn is_line_comment(line: &str) -> bool {
    line.trim_start().starts_with("//")
}

#[cfg(test)]
mod tests {
    use super::{FoldKind, folding_ranges};

    #[test]
    fn folds_bodies_import_runs_and_comment_runs() {
        let source = "import package::a::x;\nimport package::b::y;\n\n// leading\n// docs\nfn main() {\n    let value = 1;\n}\n";
        let ranges = folding_ranges(source);

        let imports = ranges
            .iter()
            .find(|range| range.kind == FoldKind::Imports)
            .unwrap();
        assert_eq!(
            &source[imports.range.clone()],
            "import package::a::x;\nimport package::b::y;"
        );

        let comment = ranges
            .iter()
            .find(|range| range.kind == FoldKind::Comment)
            .unwrap();
        assert_eq!(&source[comment.range.clone()], "// leading\n// docs");

        let region = ranges
            .iter()
            .find(|range| range.kind == FoldKind::Region)
            .unwrap();
        assert_eq!(&source[region.range.clone()], "{\n    let value = 1;\n}");
    }

    #[test]
    fn single_line_runs_are_not_foldable() {
        let source = "// only one\nimport package::a::x;\nfn main() {}\n";
        assert!(folding_ranges(source).is_empty(), "{source}");
    }

    #[test]
    fn block_comments_fold_and_braces_inside_comments_are_ignored() {
        let source = "/* opening\n   { not a real scope\n*/\nfn main() {\n    let x = 1;\n}\n";
        let ranges = folding_ranges(source);
        assert_eq!(
            ranges
                .iter()
                .filter(|range| range.kind == FoldKind::Comment)
                .count(),
            1
        );
        assert_eq!(
            ranges
                .iter()
                .filter(|range| range.kind == FoldKind::Region)
                .count(),
            1,
            "{ranges:#?}"
        );
    }

    #[test]
    fn unparseable_source_still_folds() {
        let source = "fn broken( {\n    let x = 1;\n}\n";
        assert!(wgsl_parse::parse_str(source).is_err());
        assert!(
            folding_ranges(source)
                .iter()
                .any(|range| range.kind == FoldKind::Region)
        );
    }
}
