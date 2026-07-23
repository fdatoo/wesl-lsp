use std::{collections::BTreeMap, path::Path};

use wgsl_parse::syntax::TranslationUnit;

const AST_INDENT: usize = 4;
const SOFT_WIDTH: usize = 100;

pub fn format(source: &str, indent_width: usize, path: &Path) -> Option<String> {
    let syntax = match wgsl_parse::parse_str(source) {
        Ok(syntax) => syntax,
        Err(error) => {
            log::error!(
                "refusing to format {}: source does not parse: {error}",
                path.display()
            );
            return None;
        }
    };
    let indent_width = indent_width.max(1);
    let Some(formatted) = format_once(&syntax, source, indent_width) else {
        log::error!(
            "refusing to format {}: comments could not be anchored",
            path.display()
        );
        return None;
    };
    let reparsed = match wgsl_parse::parse_str(&formatted) {
        Ok(syntax) => syntax,
        Err(error) => {
            log::error!(
                "refusing to format {}: output does not parse: {error}",
                path.display()
            );
            return None;
        }
    };
    if syntax.to_string() != reparsed.to_string() {
        log::error!("refusing to format {}: syntax tree changed", path.display());
        return None;
    }
    if format_once(&reparsed, &formatted, indent_width).as_deref() != Some(&formatted) {
        log::error!(
            "refusing to format {}: output is not idempotent",
            path.display()
        );
        return None;
    }
    Some(formatted)
}

fn format_once(syntax: &TranslationUnit, source: &str, indent_width: usize) -> Option<String> {
    let ast = disambiguate_unary_operators(&syntax.to_string());
    let ast = normalize_attributed_imports(ast);
    let ast = normalize_while_conditions(ast);
    let mut output = reindent(&ast, indent_width);
    output = output.replace("{\n\n}", "{}");
    output = add_struct_trailing_commas(&output);
    output = wrap_long_parenthesized_lists(output, indent_width);
    attach_comments(source, &output)
}
fn normalize_attributed_imports(source: String) -> String {
    let mut lines = source.lines();
    let mut output = String::with_capacity(source.len());
    while let Some(line) = lines.next() {
        let trimmed = line.trim_start();
        if let Some(attribute) = trimmed.strip_prefix("import @")
            && let Some(path) = lines.next()
        {
            let indent = &line[..line.len() - trimmed.len()];
            output.push_str(indent);
            output.push('@');
            output.push_str(attribute);
            output.push('\n');
            output.push_str(indent);
            output.push_str("import ");
            output.push_str(path.trim_start());
            output.push('\n');
        } else {
            output.push_str(line);
            output.push('\n');
        }
    }
    if !source.ends_with('\n') {
        output.pop();
    }
    output
}

fn normalize_while_conditions(mut source: String) -> String {
    let mut search = 0;
    while let Some(relative) = source[search..].find("while (") {
        let outer = search + relative + "while ".len();
        let bytes = source.as_bytes();
        let mut depth = 0usize;
        let mut matching = None;
        for (relative, byte) in bytes[outer..].iter().enumerate() {
            match byte {
                b'(' => depth += 1,
                b')' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        matching = Some(outer + relative);
                        break;
                    }
                }
                _ => {}
            }
        }
        let Some(matching) = matching else {
            break;
        };
        source.remove(matching);
        source.remove(outer);
        search = outer;
    }
    source
}

fn disambiguate_unary_operators(source: &str) -> String {
    let bytes = source.as_bytes();
    let mut output = String::with_capacity(source.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'-' {
            let start = index;
            while index < bytes.len() && bytes[index] == b'-' {
                index += 1;
            }
            let count = index - start;
            let next = bytes[index..]
                .iter()
                .copied()
                .find(|byte| !byte.is_ascii_whitespace());
            let previous = bytes[..start]
                .iter()
                .rev()
                .copied()
                .find(|byte| !byte.is_ascii_whitespace());
            let postfix = count == 2
                && previous.is_some_and(|byte| {
                    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b')' | b']')
                })
                && matches!(next, Some(b';' | b')' | b',' | b'}'));
            if count > 1 && !postfix {
                output.push_str(&"- ".repeat(count - 1));
                output.push('-');
            } else {
                output.push_str(&source[start..index]);
            }
            continue;
        }
        let character = source[index..].chars().next().unwrap();
        output.push(character);
        index += character.len_utf8();
    }
    output
}

fn reindent(source: &str, indent_width: usize) -> String {
    let mut output = String::with_capacity(source.len());
    for line in source.split_inclusive('\n') {
        let content = line.strip_suffix('\n').unwrap_or(line);
        let spaces = content.bytes().take_while(|byte| *byte == b' ').count();
        let depth = spaces / AST_INDENT;
        output.extend(std::iter::repeat_n(' ', depth * indent_width));
        output.push_str(&content[spaces..]);
        if line.ends_with('\n') {
            output.push('\n');
        }
    }
    output
}

fn add_struct_trailing_commas(source: &str) -> String {
    let mut lines: Vec<String> = source.lines().map(str::to_owned).collect();
    let mut structures = Vec::new();
    for index in 0..lines.len() {
        let trimmed = lines[index].trim();
        let indent = lines[index].len() - lines[index].trim_start().len();
        if trimmed.starts_with("struct ") && trimmed.ends_with('{') {
            structures.push((indent, index));
            continue;
        }
        if trimmed == "}"
            && let Some(position) = structures
                .iter()
                .rposition(|(structure_indent, _)| *structure_indent == indent)
        {
            let (_, start) = structures.remove(position);
            if let Some(member) = (start + 1..index)
                .rev()
                .find(|line| !lines[*line].trim().is_empty())
                && !lines[member].trim_end().ends_with(',')
            {
                lines[member].push(',');
            }
        }
    }
    let mut output = lines.join("\n");
    if source.ends_with('\n') {
        output.push('\n');
    }
    output
}

fn wrap_long_parenthesized_lists(mut source: String, indent_width: usize) -> String {
    for _ in 0..256 {
        let Some((line_start, line_end)) = source
            .split_inclusive('\n')
            .scan(0usize, |offset, line| {
                let start = *offset;
                *offset += line.len();
                Some((start, *offset, line.trim_end_matches('\n').len()))
            })
            .find_map(|(start, end, width)| {
                (width > SOFT_WIDTH)
                    .then_some((start, end - usize::from(source[..end].ends_with('\n'))))
            })
        else {
            break;
        };
        let Some((open, close, items)) = parenthesized_list(&source, line_start, line_end) else {
            break;
        };
        let base_indent = source[line_start..open]
            .bytes()
            .take_while(|byte| *byte == b' ')
            .count();
        let base = " ".repeat(base_indent);
        let child = " ".repeat(base_indent + indent_width);
        let replacement = format!(
            "(\n{child}{},\n{base})",
            items
                .into_iter()
                .map(str::trim)
                .collect::<Vec<_>>()
                .join(&format!(",\n{child}"))
        );
        source.replace_range(open..=close, &replacement);
    }
    source
}

fn parenthesized_list(
    source: &str,
    line_start: usize,
    line_end: usize,
) -> Option<(usize, usize, Vec<&str>)> {
    let bytes = source.as_bytes();
    let mut stack = Vec::new();
    let mut pairs = Vec::new();
    for (relative, byte) in bytes[line_start..line_end].iter().enumerate() {
        let index = line_start + relative;
        match byte {
            b'(' => stack.push(index),
            b')' => {
                if let Some(open) = stack.pop() {
                    pairs.push((open, index));
                }
            }
            _ => {}
        }
    }
    pairs.sort_by_key(|(open, close)| std::cmp::Reverse(close - open));
    for (open, close) in pairs {
        let items = split_top_level(&source[open + 1..close]);
        if items.len() > 1 {
            return Some((open, close, items));
        }
    }
    None
}

fn split_top_level(source: &str) -> Vec<&str> {
    let bytes = source.as_bytes();
    let mut items = Vec::new();
    let mut start = 0;
    let (mut parens, mut brackets, mut angles) = (0usize, 0usize, 0usize);
    for (index, byte) in bytes.iter().enumerate() {
        match byte {
            b'(' => parens += 1,
            b')' => parens = parens.saturating_sub(1),
            b'[' => brackets += 1,
            b']' => brackets = brackets.saturating_sub(1),
            b'<' => angles += 1,
            b'>' => angles = angles.saturating_sub(1),
            b',' if parens == 0 && brackets == 0 && angles == 0 => {
                items.push(&source[start..index]);
                start = index + 1;
            }
            _ => {}
        }
    }
    if start > 0 {
        let tail = source[start..].trim();
        if !tail.is_empty() {
            items.push(tail);
        }
    }
    items
}

#[derive(Clone, Copy)]
enum CommentAnchor {
    Before(usize),
    After(usize),
}

struct Comment<'a> {
    text: &'a str,
    anchor: CommentAnchor,
    standalone: bool,
    line: bool,
}

#[derive(Clone)]
struct Token<'a> {
    text: &'a str,
    start: usize,
    end: usize,
}

fn attach_comments(source: &str, formatted: &str) -> Option<String> {
    let (source_tokens, comments) = scan(source);
    if comments.is_empty() {
        return Some(formatted.to_owned());
    }
    let (formatted_tokens, _) = scan(formatted);
    let mapping = align_tokens(&source_tokens, &formatted_tokens)?;
    let mut insertions: BTreeMap<usize, Vec<String>> = BTreeMap::new();
    for comment in comments {
        let (offset, insertion) = match comment.anchor {
            CommentAnchor::Before(index) if index < source_tokens.len() => {
                let formatted_index = mapping[index];
                let token = &formatted_tokens[formatted_index];
                (
                    token.start,
                    leading_comment(formatted, token.start, &comment),
                )
            }
            CommentAnchor::Before(_) => (
                formatted.len(),
                leading_comment(formatted, formatted.len(), &comment),
            ),
            CommentAnchor::After(index) => {
                let formatted_index = mapping[index];
                let token = &formatted_tokens[formatted_index];
                (token.end, trailing_comment(formatted, token.end, &comment))
            }
        };
        insertions.entry(offset).or_default().push(insertion);
    }

    let mut output = String::with_capacity(
        formatted.len()
            + insertions
                .values()
                .flatten()
                .map(String::len)
                .sum::<usize>(),
    );
    let mut cursor = 0;
    for (offset, values) in insertions {
        output.push_str(&formatted[cursor..offset]);
        for value in values {
            output.push_str(&value);
        }
        cursor = offset;
    }
    output.push_str(&formatted[cursor..]);
    Some(output)
}

fn leading_comment(formatted: &str, offset: usize, comment: &Comment<'_>) -> String {
    let line_start = formatted[..offset].rfind('\n').map_or(0, |index| index + 1);
    let prefix = &formatted[line_start..offset];
    let indent: String = prefix
        .chars()
        .take_while(|character| *character == ' ')
        .collect();
    if comment.standalone || comment.line {
        if prefix.trim().is_empty() {
            format!("{}\n{indent}", comment.text)
        } else {
            format!("\n{indent}{}\n{indent}", comment.text)
        }
    } else {
        format!("{} ", comment.text)
    }
}

fn trailing_comment(formatted: &str, offset: usize, comment: &Comment<'_>) -> String {
    if !comment.line {
        return format!(" {}", comment.text);
    }
    let suffix = &formatted[offset..];
    if suffix
        .find('\n')
        .is_some_and(|newline| suffix[..newline].trim().is_empty())
    {
        format!(" {}", comment.text)
    } else {
        let indent = suffix
            .find('\n')
            .map(|newline| {
                let rest = &suffix[newline + 1..];
                rest.chars()
                    .take_while(|character| *character == ' ')
                    .collect::<String>()
            })
            .unwrap_or_default();
        format!(" {}\n{indent}", comment.text)
    }
}

fn align_tokens(source: &[Token<'_>], formatted: &[Token<'_>]) -> Option<Vec<usize>> {
    let mut mapping = vec![0; source.len()];
    let (mut left, mut right) = (0usize, 0usize);
    while left < source.len() && right < formatted.len() {
        if compatible_token(source[left].text, formatted[right].text) {
            mapping[left] = right;
            left += 1;
            right += 1;
        } else if formatted
            .get(right + 1)
            .is_some_and(|next| compatible_token(source[left].text, next.text))
        {
            right += 1;
        } else if source
            .get(left + 1)
            .is_some_and(|next| compatible_token(next.text, formatted[right].text))
            || punctuation(source[left].text)
        {
            mapping[left] = right.saturating_sub(1);
            left += 1;
        } else if punctuation(formatted[right].text) {
            right += 1;
        } else {
            log::error!(
                "comment token alignment diverged at source token {:?}, formatted token {:?}",
                source.get(left).map(|token| token.text),
                formatted.get(right).map(|token| token.text)
            );
            return None;
        }
    }
    (left == source.len()).then_some(mapping)
}

fn punctuation(token: &str) -> bool {
    token.len() == 1
        && token
            .as_bytes()
            .first()
            .is_some_and(|byte| !byte.is_ascii_alphanumeric() && *byte != b'_')
}

fn compatible_token(left: &str, right: &str) -> bool {
    left == right
        || (left.as_bytes().first().is_some_and(u8::is_ascii_digit)
            && right.as_bytes().first().is_some_and(u8::is_ascii_digit))
}

fn scan(source: &str) -> (Vec<Token<'_>>, Vec<Comment<'_>>) {
    let bytes = source.as_bytes();
    let mut tokens = Vec::new();
    let mut comments = Vec::new();
    let mut index = 0;
    let mut line_has_code = false;
    while index < bytes.len() {
        if bytes[index].is_ascii_whitespace() {
            if bytes[index] == b'\n' {
                line_has_code = false;
            }
            index += 1;
            continue;
        }
        if bytes[index..].starts_with(b"//") {
            let start = index;
            index = source[start..]
                .find('\n')
                .map_or(bytes.len(), |newline| start + newline);
            let anchor = if line_has_code && !tokens.is_empty() {
                CommentAnchor::After(tokens.len() - 1)
            } else {
                CommentAnchor::Before(tokens.len())
            };
            comments.push(Comment {
                text: &source[start..index],
                anchor,
                standalone: !line_has_code,
                line: true,
            });
            continue;
        }
        if bytes[index..].starts_with(b"/*") {
            let start = index;
            let mut depth = 1usize;
            index += 2;
            while index < bytes.len() && depth > 0 {
                if bytes[index..].starts_with(b"/*") {
                    depth += 1;
                    index += 2;
                } else if bytes[index..].starts_with(b"*/") {
                    depth -= 1;
                    index += 2;
                } else {
                    index += 1;
                }
            }
            let anchor = if line_has_code && !tokens.is_empty() {
                CommentAnchor::After(tokens.len() - 1)
            } else {
                CommentAnchor::Before(tokens.len())
            };
            comments.push(Comment {
                text: &source[start..index],
                anchor,
                standalone: !line_has_code,
                line: false,
            });
            if source[start..index].contains('\n') {
                line_has_code = false;
            }
            continue;
        }
        let start = index;
        if bytes[index].is_ascii_alphabetic() || bytes[index] == b'_' {
            index += 1;
            while index < bytes.len()
                && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_')
            {
                index += 1;
            }
        } else if bytes[index].is_ascii_digit() {
            index += 1;
            while index < bytes.len() {
                if bytes[index].is_ascii_alphanumeric()
                    || matches!(bytes[index], b'_' | b'.')
                    || (matches!(bytes[index], b'+' | b'-')
                        && matches!(bytes[index - 1], b'e' | b'E'))
                {
                    index += 1;
                } else {
                    break;
                }
            }
        } else {
            index += source[index..].chars().next().unwrap().len_utf8();
        }
        tokens.push(Token {
            text: &source[start..index],
            start,
            end: index,
        });
        line_has_code = true;
    }
    (tokens, comments)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{SOFT_WIDTH, format};

    #[test]
    fn preserves_comments_and_is_idempotent() {
        let source = "fn main() {\n// keep { this\nlet x = 1; /* and } this */\n}\n";
        let expected = "fn main() {\n    // keep { this\n    let x = 1; /* and } this */\n}\n";
        let formatted = format(source, 4, Path::new("shader.wesl")).unwrap();
        assert_eq!(formatted, expected);
        assert_eq!(
            format(&formatted, 4, Path::new("shader.wesl")).unwrap(),
            formatted
        );
    }

    #[test]
    fn collapses_excess_top_level_blank_lines() {
        let source = "fn a() {}\n\n\nfn b() {}\n";
        let expected = "fn a() {}\n\nfn b() {}\n";
        assert_eq!(
            format(source, 4, Path::new("shader.wgsl")).unwrap(),
            expected
        );
    }

    #[test]
    fn formats_from_ast_and_adds_struct_trailing_commas() {
        let source = "struct S{@location(0)x:f32,y:vec4<f32>,}\nfn main(){let x=1+2;}\n";
        let expected = "struct S {\n  @location(0)\n  x: f32,\n  y: vec4<f32>,\n}\n\nfn main() {\n  let x = 1 + 2;\n}\n";
        assert_eq!(
            format(source, 2, Path::new("shader.wgsl")).unwrap(),
            expected
        );
    }

    #[test]
    fn wraps_long_argument_lists_with_trailing_commas() {
        let source = format!(
            "fn f({}: f32, {}: f32, {}: f32) {{}}\n",
            "a".repeat(40),
            "b".repeat(40),
            "c".repeat(40)
        );
        let formatted = format(&source, 4, Path::new("shader.wgsl")).unwrap();
        assert!(formatted.starts_with("fn f(\n"));
        assert!(formatted.contains(",\n) {}"));
        assert!(formatted.lines().all(|line| line.len() <= SOFT_WIDTH));
    }

    #[test]
    fn anchors_comment_inside_expression() {
        let source = "fn main() {\nlet x = 1 +\n// reason\n2;\n}\n";
        let formatted = format(source, 4, Path::new("shader.wgsl")).unwrap();
        assert!(formatted.contains("1 + \n    // reason\n    2"));
        assert_eq!(
            format(&formatted, 4, Path::new("shader.wgsl")).unwrap(),
            formatted
        );
    }

    #[test]
    fn formats_operator_comment_regression() {
        let _ = env_logger::builder().is_test(true).try_init();
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../corpus/wesl-rs/crates/wesl-test/wgpu/in/collatz.wgsl");
        let source = std::fs::read_to_string(path).unwrap();
        let syntax = wgsl_parse::parse_str(&source).unwrap();
        let first = super::format_once(&syntax, &source, 4).expect("first comment attachment");
        let reparsed = wgsl_parse::parse_str(&first).unwrap();
        assert_eq!(
            syntax.to_string(),
            reparsed.to_string(),
            "structure changed"
        );
        let second = super::format_once(&reparsed, &first, 4).expect("second comment attachment");
        assert_eq!(first, second, "formatter is not idempotent");
    }

    #[test]
    fn rejects_invalid_source() {
        assert!(format("fn broken( {", 4, Path::new("shader.wesl")).is_none());
    }
}
