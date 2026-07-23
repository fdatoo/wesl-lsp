use std::ops::Range;

const DIRECTIVES: &[&str] = &[
    "#import",
    "#define_import_path",
    "#ifdef",
    "#ifndef",
    "#if",
    "#else",
    "#endif",
];

pub(crate) fn is_naga_oil(source: &str) -> bool {
    source.lines().any(|line| directive(line).is_some())
}

pub(crate) fn preprocess(source: &str) -> String {
    if !is_naga_oil(source) {
        return source.to_owned();
    }
    let mut bytes = source.as_bytes().to_vec();
    let mut line_start = 0;
    for line in source.split_inclusive('\n') {
        if directive(line).is_some() {
            for byte in &mut bytes[line_start..line_start + line.len()] {
                if *byte != b'\n' && *byte != b'\r' {
                    *byte = b' ';
                }
            }
        }
        line_start += line.len();
    }

    let mut cursor = 0;
    while cursor + 2 < bytes.len() {
        if bytes[cursor] == b'#'
            && bytes[cursor + 1] == b'{'
            && let Some(relative_end) = bytes[cursor + 2..].iter().position(|byte| *byte == b'}')
        {
            let end = cursor + 2 + relative_end + 1;
            let replacement = equal_length_identifier(end - cursor);
            bytes[cursor..end].copy_from_slice(replacement.as_bytes());
            cursor = end;
            continue;
        }
        cursor += 1;
    }
    String::from_utf8(bytes).expect("preprocessing preserves UTF-8")
}

pub(crate) fn imports(source: &str) -> Vec<(String, Range<usize>)> {
    scan_paths(source, "#import")
}

pub(crate) fn definitions(source: &str) -> Vec<(String, Range<usize>)> {
    scan_paths(source, "#define_import_path")
}

fn directive(line: &str) -> Option<&'static str> {
    let trimmed = line.trim_start();
    DIRECTIVES.iter().copied().find(|directive| {
        trimmed
            .strip_prefix(directive)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with(char::is_whitespace))
    })
}

fn scan_paths(source: &str, expected: &str) -> Vec<(String, Range<usize>)> {
    let mut result = Vec::new();
    let mut line_start = 0;
    for line in source.split_inclusive('\n') {
        let indentation = line.len() - line.trim_start().len();
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix(expected)
            && rest.starts_with(char::is_whitespace)
        {
            let leading = rest.len() - rest.trim_start().len();
            let path = rest.split_whitespace().next().unwrap_or("");
            if !path.is_empty() {
                let start = line_start + indentation + expected.len() + leading;
                result.push((path.to_owned(), start..start + path.len()));
            }
        }
        line_start += line.len();
    }
    result
}

fn equal_length_identifier(length: usize) -> String {
    const BASE: &str = "__naga_oil_sub";
    if length <= BASE.len() {
        BASE[..length].to_owned()
    } else {
        let mut result = String::with_capacity(length);
        result.push_str(BASE);
        result.extend(std::iter::repeat_n('_', length - BASE.len()));
        result
    }
}

#[cfg(test)]
mod tests {
    use super::{definitions, imports, is_naga_oil, preprocess};

    #[test]
    fn strips_directives_and_preserves_offsets() {
        let source =
            "#define_import_path bevy::mesh\n#import bevy::other\nfn f() { let x = #{VALUE}; }\n";
        let processed = preprocess(source);
        assert!(is_naga_oil(source));
        assert_eq!(processed.len(), source.len());
        assert_eq!(
            processed.lines().next().unwrap().len(),
            source.lines().next().unwrap().len()
        );
        assert!(wgsl_parse::parse_str(&processed).is_ok());
        assert_eq!(definitions(source)[0].0, "bevy::mesh");
        assert_eq!(imports(source)[0].0, "bevy::other");
    }
}
