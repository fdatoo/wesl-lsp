//! Inlay hints: the inferred type of a `let`/`var` that has no written annotation, and
//! parameter names at call sites.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InlayKind {
    Type,
    Parameter,
    /// Byte offset, alignment and size of a struct member.
    Layout,
}

/// A hint anchored at a byte offset. Type hints sit just after the declared name, parameter
/// hints just before the argument they label, and layout hints at the end of the member
/// declaration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InlayHint {
    pub offset: usize,
    pub label: String,
    pub kind: InlayKind,
}
