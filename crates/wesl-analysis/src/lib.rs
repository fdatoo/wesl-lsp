mod analysis;
mod builtins;
mod dialect;
mod index;
mod line_index;
mod overlay;
mod root;
mod ty;

pub use analysis::{AnalysisHost, Diagnostic, DiagnosticSeverity};
pub use builtins::{BUILTIN_FUNCTIONS, BUILTIN_TYPES, BuiltinFn, BuiltinOverload, builtin};
pub(crate) use index::PackageIndex;
pub use index::{
    Completion, CompletionKind, HoverInfo, Location, SourceEdit, Symbol, SymbolKind,
    WorkspaceSymbol,
};
pub use line_index::{LineIndex, Position};
pub use overlay::OverlayResolver;
pub use root::discover_root;
pub use ty::{Ty, TypeDiagnostic, check_module};
