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

/// Which hint kinds to compute. Checked before the work is done rather than filtering
/// afterwards: layout hints and type hints each run a type-checker pass, so computing a
/// disabled kind and discarding it would cost a full pass on every keystroke.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InlayHintConfig {
    pub type_hints: bool,
    pub parameter_hints: bool,
    /// Off by default. Layout hints annotate every member of every struct whether or not the
    /// reader is thinking about memory, and the information is only actionable while
    /// reconciling a shader struct against a host-side one.
    pub struct_layout_hints: bool,
}

impl Default for InlayHintConfig {
    fn default() -> Self {
        Self {
            type_hints: true,
            parameter_hints: true,
            struct_layout_hints: false,
        }
    }
}

impl InlayHintConfig {
    /// Nothing to compute, so callers can skip the request entirely.
    pub fn is_empty(&self) -> bool {
        !self.type_hints && !self.parameter_hints && !self.struct_layout_hints
    }
}
