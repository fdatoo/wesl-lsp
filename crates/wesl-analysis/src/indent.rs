//! Single-line re-indentation for `textDocument/onTypeFormatting`.
//!
//! Deliberately *not* routed through [`wesl_fmt::format`]: that needs a document which parses,
//! and while a line is being typed it usually does not — a faithful whole-document format
//! would silently do nothing most of the time, which reads as broken. Counting braces over the
//! token stream instead works mid-edit, in keeping with [`crate::folding`],
//! [`crate::selection`] and [`crate::signature`].

use std::ops::Range;

use crate::index::tokens;

/// The replacement for the indentation of the line containing `offset`, or `None` when it is
/// already correct, the line is blank, or the offset is out of bounds.
pub fn reindent_line(
    source: &str,
    offset: usize,
    indent_width: usize,
) -> Option<(Range<usize>, String)> {
    if offset > source.len() {
        return None;
    }
    let line_start = source[..offset].rfind('\n').map_or(0, |index| index + 1);
    let line_end = source[line_start..]
        .find('\n')
        .map_or(source.len(), |index| line_start + index);
    let line = &source[line_start..line_end];
    if line.trim().is_empty() {
        return None;
    }

    // Braces from the token stream, so ones inside comments do not shift the depth.
    let mut depth: i32 = 0;
    for (token, range) in tokens(source) {
        if range.end > line_start {
            break;
        }
        match token {
            "{" => depth += 1,
            "}" => depth -= 1,
            _ => {}
        }
    }
    // A line that closes a block belongs to the enclosing level, not the one it closes.
    if line.trim_start().starts_with('}') {
        depth -= 1;
    }

    let existing = line.len() - line.trim_start().len();
    let desired = " ".repeat(depth.max(0) as usize * indent_width.max(1));
    (line[..existing] != desired).then(|| (line_start..line_start + existing, desired))
}

#[cfg(test)]
mod tests {
    use super::reindent_line;

    /// Applies the suggestion so assertions read as the resulting line.
    fn apply(source: &str, offset: usize, indent_width: usize) -> Option<String> {
        let (range, indent) = reindent_line(source, offset, indent_width)?;
        let mut result = source.to_owned();
        result.replace_range(range, &indent);
        Some(result)
    }

    #[test]
    fn closing_brace_dedents_to_the_enclosing_level() {
        let source = "fn main() {\n    let x = 1;\n        }\n";
        let brace = source.rfind('}').unwrap();
        assert_eq!(
            apply(source, brace, 4).unwrap(),
            "fn main() {\n    let x = 1;\n}\n"
        );
    }

    #[test]
    fn body_lines_indent_one_level_per_open_brace() {
        let source = "fn main() {\nlet x = 1;\n}\n";
        let body = source.find("let x").unwrap();
        assert_eq!(
            apply(source, body, 4).unwrap(),
            "fn main() {\n    let x = 1;\n}\n"
        );

        let nested = "fn main() {\n    if true {\nlet x = 1;\n    }\n}\n";
        let inner = nested.find("let x").unwrap();
        assert_eq!(
            apply(nested, inner, 4).unwrap(),
            "fn main() {\n    if true {\n        let x = 1;\n    }\n}\n"
        );
    }

    #[test]
    fn tab_width_is_honoured() {
        let source = "fn main() {\nlet x = 1;\n}\n";
        let body = source.find("let x").unwrap();
        assert_eq!(
            apply(source, body, 2).unwrap(),
            "fn main() {\n  let x = 1;\n}\n"
        );
    }

    #[test]
    fn already_correct_lines_are_left_alone() {
        let source = "fn main() {\n    let x = 1;\n}\n";
        assert!(reindent_line(source, source.find("let x").unwrap(), 4).is_none());
        assert!(reindent_line(source, source.rfind('}').unwrap(), 4).is_none());
    }

    #[test]
    fn blank_lines_and_out_of_bounds_offsets_are_ignored() {
        let source = "fn main() {\n\n}\n";
        assert!(reindent_line(source, source.find("\n\n").unwrap() + 1, 4).is_none());
        assert!(reindent_line(source, 9999, 4).is_none());
    }

    #[test]
    fn braces_in_comments_do_not_shift_the_depth() {
        let source = "// a stray { brace\nfn main() {\nlet x = 1;\n}\n";
        let body = source.find("let x").unwrap();
        assert_eq!(
            apply(source, body, 4).unwrap(),
            "// a stray { brace\nfn main() {\n    let x = 1;\n}\n"
        );
    }

    #[test]
    fn unparseable_source_still_indents() {
        let source = "fn broken( {\nlet x = 1;\n}\n";
        assert!(wgsl_parse::parse_str(source).is_err());
        let body = source.find("let x").unwrap();
        assert_eq!(
            apply(source, body, 4).unwrap(),
            "fn broken( {\n    let x = 1;\n}\n"
        );
    }
}
