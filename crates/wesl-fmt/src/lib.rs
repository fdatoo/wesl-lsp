use std::path::Path;

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
    let formatted = format_once(source, indent_width.max(1));
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
    if format_once(&formatted, indent_width.max(1)) != formatted {
        log::error!(
            "refusing to format {}: output is not idempotent",
            path.display()
        );
        return None;
    }
    Some(formatted)
}

fn format_once(source: &str, indent_width: usize) -> String {
    let mut output = String::with_capacity(source.len());
    let mut depth = 0usize;
    let mut in_block_comment = false;
    for line in source.split_inclusive('\n') {
        let had_newline = line.ends_with('\n');
        let content = line.strip_suffix('\n').unwrap_or(line);
        let content = content.strip_suffix('\r').unwrap_or(content);
        let trimmed = content.trim();
        if trimmed.is_empty() {
            if had_newline {
                output.push('\n');
            }
            continue;
        }
        let (opens, closes, starts_with_close) = brace_counts(trimmed, &mut in_block_comment);
        let line_depth = depth.saturating_sub(usize::from(starts_with_close));
        output.extend(std::iter::repeat_n(' ', line_depth * indent_width));
        output.push_str(trimmed);
        if had_newline {
            output.push('\n');
        }
        depth = depth.saturating_sub(closes).saturating_add(opens);
    }
    output
}

fn brace_counts(line: &str, in_block_comment: &mut bool) -> (usize, usize, bool) {
    let bytes = line.as_bytes();
    let mut opens = 0;
    let mut closes = 0;
    let mut first_code = None;
    let mut index = 0;
    while index < bytes.len() {
        if *in_block_comment {
            if index + 1 < bytes.len() && bytes[index..].starts_with(b"*/") {
                *in_block_comment = false;
                index += 2;
            } else {
                index += 1;
            }
            continue;
        }
        if index + 1 < bytes.len() && bytes[index..].starts_with(b"//") {
            break;
        }
        if index + 1 < bytes.len() && bytes[index..].starts_with(b"/*") {
            *in_block_comment = true;
            index += 2;
            continue;
        }
        match bytes[index] {
            b'{' => {
                first_code.get_or_insert(b'{');
                opens += 1;
            }
            b'}' => {
                first_code.get_or_insert(b'}');
                closes += 1;
            }
            byte if !byte.is_ascii_whitespace() => {
                first_code.get_or_insert(byte);
            }
            _ => {}
        }
        index += 1;
    }
    (opens, closes, first_code == Some(b'}'))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::format;

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
    fn rejects_invalid_source() {
        assert!(format("fn broken( {", 4, Path::new("shader.wesl")).is_none());
    }
}
