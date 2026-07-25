//! Inlay hints: the inferred type of a `let`/`var` that has no written annotation, and
//! parameter names at call sites.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InlayKind {
    Type,
    Parameter,
}

/// A hint anchored at a byte offset. Type hints sit just after the declared name, parameter
/// hints just before the argument they label.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InlayHint {
    pub offset: usize,
    pub label: String,
    pub kind: InlayKind,
}
