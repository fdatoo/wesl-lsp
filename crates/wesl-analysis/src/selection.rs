//! "Expand selection" chains. Built from token and delimiter nesting rather than the AST,
//! for the same reason as [`crate::folding`]: the cursor is often sitting in a buffer that
//! does not currently parse, and expanding a selection should still work there.
//!
//! The consequence is that granularity stops at delimiters — expanding inside `a + b * c`
//! jumps straight from the identifier to the enclosing bracket, rather than stepping through
//! sub-expressions the way an AST walk would.

use std::ops::Range;

use crate::index::tokens;

/// Ranges containing `offset`, innermost first, each strictly containing the one before it.
pub fn selection_ranges(source: &str, offset: usize) -> Vec<Range<usize>> {
    if offset > source.len() {
        return Vec::new();
    }
    let mut candidates = Vec::new();
    if let Some(token) = token_at(source, offset) {
        candidates.push(token);
    }
    candidates.extend(statement_at(source, offset));
    candidates.extend(delimited_at(source, offset));
    candidates.push(0..source.len());

    candidates.sort_by_key(|range| (range.end - range.start, range.start));
    let mut chain: Vec<Range<usize>> = Vec::new();
    for candidate in candidates {
        let grows = chain.last().is_none_or(|last| {
            candidate.start <= last.start && candidate.end >= last.end && candidate != *last
        });
        if grows {
            chain.push(candidate);
        }
    }
    chain
}

fn token_at(source: &str, offset: usize) -> Option<Range<usize>> {
    let tokens = tokens(source);
    tokens
        .iter()
        .find(|(_, range)| range.start <= offset && offset < range.end)
        .or_else(|| tokens.iter().find(|(_, range)| range.end == offset))
        .map(|(_, range)| range.clone())
}

/// Every balanced delimiter pair enclosing `offset`, contributing both the contents and the
/// contents-plus-delimiters so that expanding steps through `a, b` before `(a, b)`.
fn delimited_at(source: &str, offset: usize) -> Vec<Range<usize>> {
    let mut stack = Vec::new();
    let mut ranges = Vec::new();
    for (token, range) in tokens(source) {
        match token {
            "(" | "[" | "{" => stack.push((token, range.start)),
            ")" | "]" | "}" => {
                let expected = match token {
                    ")" => "(",
                    "]" => "[",
                    _ => "{",
                };
                if let Some(position) = stack.iter().rposition(|(open, _)| *open == expected) {
                    let (_, start) = stack.remove(position);
                    stack.truncate(position);
                    let outer = start..range.end;
                    if outer.start <= offset && offset <= outer.end {
                        let inner = start + 1..range.start;
                        if inner.start <= offset && offset <= inner.end {
                            ranges.push(inner);
                        }
                        ranges.push(outer);
                    }
                }
            }
            _ => {}
        }
    }
    ranges
}

/// The run between statement boundaries — `;`, `{` or `}` — around `offset`, trimmed of
/// surrounding whitespace.
fn statement_at(source: &str, offset: usize) -> Option<Range<usize>> {
    let boundaries = tokens(source)
        .into_iter()
        .filter(|(token, _)| matches!(*token, ";" | "{" | "}"))
        .map(|(_, range)| range)
        .collect::<Vec<_>>();
    let start = boundaries
        .iter()
        .filter(|range| range.end <= offset)
        .map(|range| range.end)
        .max()
        .unwrap_or(0);
    let end = boundaries
        .iter()
        .filter(|range| range.start >= offset)
        .map(|range| {
            if source[range.clone()] == *";" {
                range.end
            } else {
                range.start
            }
        })
        .min()
        .unwrap_or(source.len());
    let trimmed_start = start + (source[start..end].len() - source[start..end].trim_start().len());
    let trimmed_end = trimmed_start + source[trimmed_start..end].trim_end().len();
    (trimmed_start < trimmed_end && trimmed_start <= offset && offset <= trimmed_end)
        .then_some(trimmed_start..trimmed_end)
}

#[cfg(test)]
mod tests {
    use super::selection_ranges;

    #[test]
    fn expands_from_identifier_through_call_to_statement() {
        let source = "fn main() {\n    let value = clamp(alpha, 0.0, 1.0);\n}\n";
        let alpha = source.find("alpha").unwrap();
        let ranges = selection_ranges(source, alpha);
        let selected = ranges
            .iter()
            .map(|range| &source[range.clone()])
            .collect::<Vec<_>>();

        assert_eq!(selected[0], "alpha");
        assert!(
            selected.contains(&"alpha, 0.0, 1.0"),
            "argument list contents: {selected:#?}"
        );
        assert!(
            selected.contains(&"(alpha, 0.0, 1.0)"),
            "argument list with parentheses: {selected:#?}"
        );
        assert!(
            selected.contains(&"let value = clamp(alpha, 0.0, 1.0);"),
            "statement: {selected:#?}"
        );
        assert_eq!(*selected.last().unwrap(), source);
    }

    #[test]
    fn each_range_strictly_contains_the_previous() {
        let source = "struct Camera { projection: mat4x4<f32>, }\nfn main() { let m = camera.projection; }\n";
        for offset in 0..source.len() {
            let ranges = selection_ranges(source, offset);
            for pair in ranges.windows(2) {
                assert!(
                    pair[1].start <= pair[0].start && pair[1].end >= pair[0].end,
                    "at offset {offset}: {:?} does not contain {:?}",
                    pair[1],
                    pair[0]
                );
                assert!(pair[1] != pair[0], "duplicate range at offset {offset}");
            }
        }
    }

    #[test]
    fn unparseable_source_still_expands() {
        let source = "fn broken( {\n    let x = alpha;\n}\n";
        assert!(wgsl_parse::parse_str(source).is_err());
        let alpha = source.find("alpha").unwrap();
        let ranges = selection_ranges(source, alpha);
        assert_eq!(&source[ranges[0].clone()], "alpha");
        assert!(ranges.len() > 1, "{ranges:#?}");
    }

    #[test]
    fn offset_past_the_end_yields_nothing() {
        assert!(selection_ranges("fn main() {}", 999).is_empty());
    }
}
