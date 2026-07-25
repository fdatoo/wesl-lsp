//! Call-site detection and signature label splitting for `textDocument/signatureHelp`.
//!
//! Like [`crate::folding`] and [`crate::selection`], this reads tokens rather than the AST:
//! signature help is requested precisely while a call is half-typed and the buffer does not
//! parse.

use std::ops::Range;

use crate::index::{is_identifier, tokens};

/// Words that take a parenthesised operand but are not calls.
const CONTROL_FLOW: &[&str] = &[
    "if", "for", "while", "switch", "return", "case", "loop", "else", "break", "continue",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignatureInfo {
    pub label: String,
    /// Byte ranges into `label`, one per parameter.
    pub parameters: Vec<Range<usize>>,
    pub documentation: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignatureHelp {
    pub signatures: Vec<SignatureInfo>,
    pub active_signature: usize,
    pub active_parameter: usize,
}

/// The innermost call enclosing `offset`, as `(callee, active parameter index)`.
///
/// Parenthesised groupings contribute nesting but no callee, so a cursor inside
/// `f(a, (b + c))` still reports `f` with the argument index counted in `f`'s own frame.
pub fn enclosing_call(source: &str, offset: usize) -> Option<(String, usize)> {
    let mut stack: Vec<(Option<String>, usize)> = Vec::new();
    let mut previous: Option<&str> = None;
    for (token, range) in tokens(source) {
        if range.start >= offset {
            break;
        }
        match token {
            "(" => {
                let callee = previous
                    .filter(|name| is_identifier(name) && !CONTROL_FLOW.contains(name))
                    .map(str::to_owned);
                stack.push((callee, 0));
            }
            ")" => {
                stack.pop();
            }
            "," => {
                if let Some((_, arguments)) = stack.last_mut() {
                    *arguments += 1;
                }
            }
            _ => {}
        }
        previous = Some(token);
    }
    stack
        .into_iter()
        .rev()
        .find_map(|(callee, arguments)| callee.map(|name| (name, arguments)))
}

/// A resolved call in the source, with the byte offset where each argument begins.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallSite {
    pub callee: String,
    pub callee_range: Range<usize>,
    pub arguments: Vec<usize>,
}

/// Every call in `source`, innermost first by completion order. Parenthesised groupings
/// nest but produce no call, matching [`enclosing_call`].
pub fn call_sites(source: &str) -> Vec<CallSite> {
    struct Frame {
        callee: Option<(String, Range<usize>)>,
        arguments: Vec<usize>,
        awaiting_argument: bool,
    }

    let mut stack: Vec<Frame> = Vec::new();
    let mut calls = Vec::new();
    let mut previous: Option<(&str, Range<usize>)> = None;
    for (token, range) in tokens(source) {
        // Any token that opens an argument marks where that argument starts.
        if !matches!(token, ")" | ",")
            && let Some(frame) = stack.last_mut()
            && frame.awaiting_argument
        {
            frame.arguments.push(range.start);
            frame.awaiting_argument = false;
        }
        match token {
            "(" => stack.push(Frame {
                callee: previous
                    .clone()
                    .filter(|(name, _)| is_identifier(name) && !CONTROL_FLOW.contains(name))
                    .map(|(name, range)| (name.to_owned(), range)),
                arguments: Vec::new(),
                awaiting_argument: true,
            }),
            ")" => {
                if let Some(frame) = stack.pop()
                    && let Some((callee, callee_range)) = frame.callee
                {
                    calls.push(CallSite {
                        callee,
                        callee_range,
                        arguments: frame.arguments,
                    });
                }
            }
            "," => {
                if let Some(frame) = stack.last_mut() {
                    frame.awaiting_argument = true;
                }
            }
            _ => {}
        }
        previous = Some((token, range));
    }
    calls
}

/// Parameter names pulled out of a signature label, e.g. `e` from `e: T`.
pub fn parameter_names(label: &str) -> Vec<String> {
    parameter_spans(label)
        .into_iter()
        .map(|span| {
            label[span]
                .split(':')
                .next()
                .unwrap_or_default()
                .trim()
                .to_owned()
        })
        .collect()
}

/// Byte ranges of each parameter inside a signature label, found by splitting the first
/// parenthesised group on commas that sit at nesting depth zero. Template arguments are
/// tracked too, so `array<f32, 4>` stays one parameter.
pub fn parameter_spans(label: &str) -> Vec<Range<usize>> {
    let Some(open) = label.find('(') else {
        return Vec::new();
    };
    let bytes = label.as_bytes();
    let mut depth = 0i32;
    let mut templates = 0i32;
    let mut spans = Vec::new();
    let mut start = open + 1;
    for (index, byte) in bytes.iter().enumerate().skip(open) {
        match *byte {
            b'(' | b'[' => depth += 1,
            b']' => depth -= 1,
            b'<' => templates += 1,
            b'>' => templates -= 1,
            b',' if depth == 1 && templates == 0 => {
                spans.extend(trimmed_span(label, start..index));
                start = index + 1;
            }
            b')' => {
                depth -= 1;
                if depth == 0 {
                    spans.extend(trimmed_span(label, start..index));
                    break;
                }
            }
            _ => {}
        }
    }
    spans
}

fn trimmed_span(label: &str, range: Range<usize>) -> Option<Range<usize>> {
    let slice = &label[range.clone()];
    let leading = slice.len() - slice.trim_start().len();
    let start = range.start + leading;
    let end = start + slice.trim().len();
    (start < end).then_some(start..end)
}

#[cfg(test)]
mod tests {
    use super::{enclosing_call, parameter_spans};

    #[test]
    fn finds_callee_and_argument_index() {
        let source = "fn main() { let x = clamp(alpha, 0.0, 1.0); }";
        let first = source.find("alpha").unwrap();
        assert_eq!(enclosing_call(source, first), Some(("clamp".into(), 0)));

        let third = source.find("1.0").unwrap();
        assert_eq!(enclosing_call(source, third), Some(("clamp".into(), 2)));
    }

    #[test]
    fn nested_calls_report_the_innermost() {
        let source = "fn main() { let x = max(min(a, b), c); }";
        let inner = source.find(" b)").unwrap();
        assert_eq!(enclosing_call(source, inner), Some(("min".into(), 1)));

        let outer = source.find(" c)").unwrap();
        assert_eq!(enclosing_call(source, outer), Some(("max".into(), 1)));
    }

    #[test]
    fn groupings_do_not_shadow_the_enclosing_call() {
        let source = "fn main() { let x = mix(a, (b + c), d); }";
        let grouped = source.find("+ c").unwrap();
        assert_eq!(enclosing_call(source, grouped), Some(("mix".into(), 1)));
    }

    #[test]
    fn control_flow_parentheses_are_not_calls() {
        let source = "fn main() { if (alpha > 0.0) { } }";
        assert_eq!(enclosing_call(source, source.find("alpha").unwrap()), None);
    }

    #[test]
    fn unterminated_call_still_resolves() {
        let source = "fn main() { let x = clamp(";
        assert_eq!(
            enclosing_call(source, source.len()),
            Some(("clamp".into(), 0))
        );
    }

    #[test]
    fn splits_parameters_and_keeps_template_commas_together() {
        let label = "@const @must_use fn clamp ( e: T, low: T, high: T ) -> T";
        let spans = parameter_spans(label);
        let parameters = spans
            .iter()
            .map(|span| &label[span.clone()])
            .collect::<Vec<_>>();
        assert_eq!(parameters, vec!["e: T", "low: T", "high: T"]);

        let templated = "fn write(target: array<f32, 4>, value: f32) -> void";
        let spans = parameter_spans(templated);
        let parameters = spans
            .iter()
            .map(|span| &templated[span.clone()])
            .collect::<Vec<_>>();
        assert_eq!(parameters, vec!["target: array<f32, 4>", "value: f32"]);
    }

    #[test]
    fn call_sites_record_each_argument_start() {
        use super::call_sites;
        let source = "fn main() { let x = mix(a, g(b), c); }";
        let calls = call_sites(source);

        let outer = calls.iter().find(|call| call.callee == "mix").unwrap();
        let starts = outer
            .arguments
            .iter()
            .map(|start| &source[*start..*start + 1])
            .collect::<Vec<_>>();
        assert_eq!(starts, vec!["a", "g", "c"]);

        let inner = calls.iter().find(|call| call.callee == "g").unwrap();
        assert_eq!(inner.arguments.len(), 1);
        assert_eq!(&source[inner.callee_range.clone()], "g");

        // A parenthesised group still opens an argument of the enclosing call.
        let grouped = call_sites("fn main() { f((a + b), c); }");
        assert_eq!(
            grouped
                .iter()
                .find(|c| c.callee == "f")
                .unwrap()
                .arguments
                .len(),
            2
        );

        assert!(call_sites("fn main() { f(); }")[0].arguments.is_empty());
    }

    #[test]
    fn parameter_names_drop_their_types() {
        use super::parameter_names;
        assert_eq!(
            parameter_names("@const @must_use fn clamp ( e: T, low: T, high: T ) -> T"),
            vec!["e", "low", "high"]
        );
    }

    #[test]
    fn empty_parameter_list_yields_nothing() {
        assert!(parameter_spans("fn main ( ) -> void").is_empty());
        assert!(parameter_spans("no parentheses here").is_empty());
    }
}
