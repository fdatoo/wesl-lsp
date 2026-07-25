mod analysis;
mod builtins;
mod dialect;
mod folding;
mod indent;
mod index;
mod inlay;
mod layout;
mod line_index;
mod overlay;
mod root;
mod selection;
mod signature;
mod ty;

pub use analysis::{AnalysisHost, Diagnostic, DiagnosticSeverity};
pub use builtins::{BUILTIN_FUNCTIONS, BUILTIN_TYPES, BuiltinFn, BuiltinOverload, builtin};
pub use folding::{FoldKind, FoldingRange, folding_ranges};
pub use indent::reindent_line;
pub(crate) use index::PackageIndex;
pub use index::{
    Completion, CompletionKind, HoverInfo, Location, SourceEdit, Symbol, SymbolKind,
    WorkspaceSymbol,
};
pub use inlay::{InlayHint, InlayHintConfig, InlayKind};
pub use layout::{MemberLayout, MemberOverrides, align_of, size_of, struct_layout};
pub use line_index::{LineIndex, Position, PositionEncoding};
pub use overlay::OverlayResolver;
pub use root::discover_root;
pub use selection::selection_ranges;
pub use signature::{SignatureHelp, SignatureInfo};
pub use ty::{Ty, TypeDiagnostic, check_module};
