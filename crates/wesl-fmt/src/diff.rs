//! Line-level diff, used to narrow whole-document formatting down to a requested range.
//!
//! [`format`](crate::format) is whole-document by construction and refuses to act when it is
//! unsure, so range formatting runs it unchanged over the full text and then returns only the
//! hunks touching the range. That keeps every safety check the formatter performs while still
//! honouring the client's request to leave the rest of the file alone.

use std::ops::Range;

/// A replacement of `range` in the original text. An empty range is a pure insertion.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hunk {
    pub range: Range<usize>,
    pub new_text: String,
}

/// Above this many changed lines on either side, the whole changed region collapses into one
/// hunk rather than allocating a quadratic table. Formatting a shader rarely gets near it.
const MAX_ALIGNED_LINES: usize = 2000;

/// Minimal per-line replacements turning `before` into `after`.
pub fn line_hunks(before: &str, after: &str) -> Vec<Hunk> {
    let before_lines = before.split_inclusive('\n').collect::<Vec<_>>();
    let after_lines = after.split_inclusive('\n').collect::<Vec<_>>();
    let offsets = line_offsets(&before_lines);

    // Trim the matching head and tail; formatting usually perturbs a small middle.
    let mut start = 0;
    while start < before_lines.len()
        && start < after_lines.len()
        && before_lines[start] == after_lines[start]
    {
        start += 1;
    }
    let mut end_before = before_lines.len();
    let mut end_after = after_lines.len();
    while end_before > start
        && end_after > start
        && before_lines[end_before - 1] == after_lines[end_after - 1]
    {
        end_before -= 1;
        end_after -= 1;
    }
    if start == end_before && start == end_after {
        return Vec::new();
    }

    let left = &before_lines[start..end_before];
    let right = &after_lines[start..end_after];
    if left.len().saturating_mul(right.len()) > MAX_ALIGNED_LINES * MAX_ALIGNED_LINES {
        return vec![Hunk {
            range: offsets[start]..offsets[end_before],
            new_text: right.concat(),
        }];
    }

    aligned_hunks(left, right, &offsets[start..])
}

/// Longest-common-subsequence alignment of the changed middle, so untouched lines between two
/// edits stay untouched instead of being swallowed by one oversized hunk.
fn aligned_hunks(left: &[&str], right: &[&str], offsets: &[usize]) -> Vec<Hunk> {
    let (rows, columns) = (left.len(), right.len());
    let width = columns + 1;
    let mut common = vec![0u32; (rows + 1) * width];
    for row in (0..rows).rev() {
        for column in (0..columns).rev() {
            common[row * width + column] = if left[row] == right[column] {
                common[(row + 1) * width + column + 1] + 1
            } else {
                common[(row + 1) * width + column].max(common[row * width + column + 1])
            };
        }
    }

    let mut hunks = Vec::new();
    let mut pending: Option<Hunk> = None;
    let (mut row, mut column) = (0, 0);
    while row < rows || column < columns {
        if row < rows && column < columns && left[row] == right[column] {
            hunks.extend(pending.take());
            row += 1;
            column += 1;
        } else if column < columns
            && (row == rows
                || common[row * width + column + 1] >= common[(row + 1) * width + column])
        {
            // An inserted line: extends the replacement text without consuming a source line.
            pending
                .get_or_insert_with(|| Hunk {
                    range: offsets[row]..offsets[row],
                    new_text: String::new(),
                })
                .new_text
                .push_str(right[column]);
            column += 1;
        } else {
            let hunk = pending.get_or_insert_with(|| Hunk {
                range: offsets[row]..offsets[row],
                new_text: String::new(),
            });
            hunk.range.end = offsets[row + 1];
            row += 1;
        }
    }
    hunks.extend(pending);
    hunks
}

fn line_offsets(lines: &[&str]) -> Vec<usize> {
    let mut offsets = Vec::with_capacity(lines.len() + 1);
    let mut offset = 0;
    for line in lines {
        offsets.push(offset);
        offset += line.len();
    }
    offsets.push(offset);
    offsets
}

#[cfg(test)]
mod tests {
    use super::{Hunk, line_hunks};

    /// Applying every hunk to `before` must reproduce `after` exactly.
    fn apply(before: &str, hunks: &[Hunk]) -> String {
        let mut result = String::new();
        let mut cursor = 0;
        for hunk in hunks {
            result.push_str(&before[cursor..hunk.range.start]);
            result.push_str(&hunk.new_text);
            cursor = hunk.range.end;
        }
        result.push_str(&before[cursor..]);
        result
    }

    #[test]
    fn identical_text_produces_nothing() {
        assert!(line_hunks("fn main() {}\n", "fn main() {}\n").is_empty());
        assert!(line_hunks("", "").is_empty());
    }

    #[test]
    fn distant_edits_stay_separate_hunks() {
        let before = "fn a() {\n      let x = 1;\n}\nfn b() {\n    let y = 2;\n}\nfn c() {\n      let z = 3;\n}\n";
        let after = "fn a() {\n    let x = 1;\n}\nfn b() {\n    let y = 2;\n}\nfn c() {\n    let z = 3;\n}\n";
        let hunks = line_hunks(before, after);

        assert_eq!(
            hunks.len(),
            2,
            "untouched middle must not be swallowed: {hunks:#?}"
        );
        assert_eq!(apply(before, &hunks), after);
        // The second hunk starts after `fn b`, proving the gap survived.
        assert!(hunks[1].range.start > before.find("fn c").unwrap() - 12);
    }

    #[test]
    fn insertions_and_deletions_round_trip() {
        let before = "one\ntwo\nthree\n";
        let after = "one\ninserted\ntwo\n";
        let hunks = line_hunks(before, after);
        assert_eq!(apply(before, &hunks), after);

        // A pure insertion carries an empty range.
        assert!(hunks.iter().any(|hunk| hunk.range.is_empty()));
    }

    #[test]
    fn trailing_and_leading_changes_round_trip() {
        for (before, after) in [
            ("a\nb\nc\n", "a\nb\n"),
            ("a\nb\n", "a\nb\nc\n"),
            ("a\nb\nc\n", "x\nb\nc\n"),
            ("a\nb\nc\n", "a\nb\nz\n"),
            ("", "new\n"),
            ("old\n", ""),
        ] {
            let hunks = line_hunks(before, after);
            assert_eq!(apply(before, &hunks), after, "{before:?} -> {after:?}");
        }
    }

    #[test]
    fn hunks_are_ordered_and_disjoint() {
        let before = "a\n  b\nc\n  d\ne\n  f\n";
        let after = "a\nb\nc\nd\ne\nf\n";
        let hunks = line_hunks(before, after);
        assert_eq!(apply(before, &hunks), after);
        for pair in hunks.windows(2) {
            assert!(
                pair[0].range.end <= pair[1].range.start,
                "overlapping hunks: {hunks:#?}"
            );
        }
    }
}
