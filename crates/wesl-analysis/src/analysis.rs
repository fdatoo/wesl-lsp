use std::{
    collections::HashMap,
    ops::Range,
    path::{Path, PathBuf},
    sync::Arc,
};

use wgsl_parse::{parse_str, syntax::TranslationUnit};

use crate::{
    Completion, HoverInfo, Location, OverlayResolver, PackageIndex, SourceEdit, Symbol, dialect,
    discover_root,
};
use wesl::Resolver;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub range: Range<usize>,
    pub severity: DiagnosticSeverity,
    pub message: String,
    pub related: Vec<(PathBuf, Range<usize>, String)>,
}

#[derive(Clone)]
struct Document {
    source: Arc<str>,
    last_good: Option<Arc<TranslationUnit>>,
    parse_error: Option<wgsl_parse::Error>,
    dialect: bool,
}

impl Document {
    fn parse(source: String, previous: Option<&Document>) -> Self {
        let source: Arc<str> = source.into();
        let dialect = dialect::is_naga_oil(&source);
        let parsed_source = if dialect {
            dialect::preprocess(&source)
        } else {
            source.to_string()
        };
        match parse_str(&parsed_source) {
            Ok(module) => Self {
                source,
                last_good: Some(Arc::new(module)),
                parse_error: None,
                dialect,
            },
            Err(error) => Self {
                source,
                last_good: previous.and_then(|document| document.last_good.clone()),
                parse_error: Some(error),
                dialect,
            },
        }
    }
}

#[derive(Clone, Default)]
pub struct AnalysisHost {
    root_override: Option<PathBuf>,
    documents: HashMap<PathBuf, Document>,
    packages: HashMap<PathBuf, PackageIndex>,
}

impl AnalysisHost {
    pub fn new(root_override: Option<PathBuf>) -> Self {
        Self {
            root_override: root_override.map(|root| root.canonicalize().unwrap_or(root)),
            documents: HashMap::new(),
            packages: HashMap::new(),
        }
    }

    pub fn set_root_override(&mut self, root: Option<PathBuf>) {
        self.root_override = root.map(|root| root.canonicalize().unwrap_or(root));
        self.packages.clear();
    }

    pub fn open(&mut self, path: PathBuf, source: String) {
        let document = Document::parse(source, self.documents.get(&path));
        let indexed_source = document.source.clone();
        self.documents.insert(path.clone(), document);
        self.update_cached_packages(&path, indexed_source);
    }

    pub fn change(&mut self, path: &Path, source: String) {
        let document = Document::parse(source, self.documents.get(path));
        let indexed_source = document.source.clone();
        self.documents.insert(path.to_path_buf(), document);
        self.update_cached_packages(path, indexed_source);
    }

    pub fn close(&mut self, path: &Path) {
        self.documents.remove(path);
        if let Ok(source) = std::fs::read_to_string(path) {
            self.update_cached_packages(path, Arc::<str>::from(source));
        }
    }

    pub fn source(&self, path: &Path) -> Option<&str> {
        self.documents
            .get(path)
            .map(|document| document.source.as_ref())
    }

    pub fn root_for(&self, path: &Path) -> PathBuf {
        discover_root(path, self.root_override.as_deref())
    }

    pub fn has_last_good_ast(&self, path: &Path) -> bool {
        self.documents
            .get(path)
            .is_some_and(|document| document.last_good.is_some())
    }

    pub fn diagnostics(&mut self, path: &Path) -> Vec<Diagnostic> {
        let Some(document) = self.documents.get(path).cloned() else {
            return Vec::new();
        };
        if let Some(error) = &document.parse_error {
            return vec![Diagnostic {
                range: clamp_range(error.span.range(), document.source.len()),
                severity: DiagnosticSeverity::Error,
                message: error.error.to_string(),
                related: Vec::new(),
            }];
        }

        if document.dialect {
            return Vec::new();
        }
        let root = self.root_for(path);
        let mut resolver = OverlayResolver::new(&root);
        for (buffer_path, buffer) in &self.documents {
            if buffer_path.starts_with(&root) {
                resolver.set_buffer(buffer_path.clone(), buffer.source.to_string());
            }
        }

        let Some(module) = document.last_good.as_ref() else {
            return Vec::new();
        };
        let import_spans = import_statement_spans(&document.source);
        let mut diagnostics = Vec::new();
        for (index, import) in module.imports.iter().enumerate() {
            let Some(module_path) = &import.path else {
                continue;
            };
            if let Err(error) = resolver.resolve_source(module_path) {
                diagnostics.push(Diagnostic {
                    range: import_spans
                        .get(index)
                        .cloned()
                        .unwrap_or(0..document.source.len().min(1)),
                    severity: DiagnosticSeverity::Error,
                    message: error.to_string(),
                    related: Vec::new(),
                });
            }
        }
        if diagnostics.is_empty()
            && let Some(cycle) = self.ensure_package(path).import_cycle(path)
        {
            let names = cycle
                .iter()
                .filter_map(|path| path.file_stem().and_then(|name| name.to_str()))
                .collect::<Vec<_>>()
                .join(" -> ");
            diagnostics.push(Diagnostic {
                range: import_spans
                    .first()
                    .cloned()
                    .unwrap_or(0..document.source.len().min(1)),
                severity: DiagnosticSeverity::Error,
                message: format!("cyclic import: {names}"),
                related: Vec::new(),
            });
        }
        if diagnostics.is_empty()
            && let Some(root_module) = resolver.module_path(path)
        {
            let options = wesl::CompileOptions {
                strip: false,
                lazy: false,
                ..wesl::CompileOptions::default()
            };
            if let Err(error) =
                wesl::compile_sourcemap(&root_module, &resolver, &wesl::NoMangler, &options)
            {
                diagnostics.push(Diagnostic {
                    range: wesl_error_span(&error)
                        .map(|range| clamp_range(range, document.source.len()))
                        .unwrap_or(0..document.source.len().min(1)),
                    severity: DiagnosticSeverity::Error,
                    message: error.to_string(),
                    related: Vec::new(),
                });
            }
        }
        if diagnostics.is_empty() {
            diagnostics.extend(crate::check_module(module).into_iter().map(|diagnostic| {
                Diagnostic {
                    range: clamp_range(diagnostic.range, document.source.len()),
                    severity: DiagnosticSeverity::Error,
                    message: diagnostic.message,
                    related: diagnostic
                        .related
                        .into_iter()
                        .map(|(range, message)| (path.to_path_buf(), range, message))
                        .collect(),
                }
            }));
        }
        diagnostics
    }

    pub fn definition(&mut self, path: &Path, offset: usize) -> Option<Location> {
        self.ensure_package(path).definition(path, offset)
    }

    pub fn references(
        &mut self,
        path: &Path,
        offset: usize,
        include_declaration: bool,
    ) -> Vec<Location> {
        self.ensure_package(path)
            .references(path, offset, include_declaration)
    }

    pub fn rename(
        &mut self,
        path: &Path,
        offset: usize,
        new_name: &str,
    ) -> Result<Vec<SourceEdit>, &'static str> {
        self.ensure_package(path).rename(path, offset, new_name)
    }

    pub fn document_symbols(&mut self, path: &Path) -> Vec<Symbol> {
        self.ensure_package(path).document_symbols(path)
    }

    pub fn hover(&mut self, path: &Path, offset: usize) -> Option<HoverInfo> {
        self.ensure_package(path).hover(path, offset)
    }

    pub fn completions(&mut self, path: &Path, offset: usize) -> Vec<Completion> {
        self.ensure_package(path).completions(path, offset)
    }

    fn ensure_package(&mut self, path: &Path) -> &mut PackageIndex {
        let root = self.root_for(path);
        if !self.packages.contains_key(&root) {
            let overlays = self
                .documents
                .iter()
                .filter(|(path, _)| path.starts_with(&root))
                .map(|(path, document)| (path.clone(), document.source.clone()))
                .collect();
            self.packages
                .insert(root.clone(), PackageIndex::build(root.clone(), &overlays));
        }
        self.packages.get_mut(&root).unwrap()
    }

    fn update_cached_packages(&mut self, path: &Path, source: Arc<str>) {
        for (root, package) in &mut self.packages {
            if path.starts_with(root) {
                package.update(path.to_path_buf(), source.clone());
            }
        }
    }
}

fn wesl_error_span(error: &wesl::Error) -> Option<Range<usize>> {
    match error {
        wesl::Error::ParseError(error) => Some(error.span.range()),
        wesl::Error::Error(diagnostic) => diagnostic.detail.span.map(|span| span.range()),
        _ => None,
    }
}

fn clamp_range(range: Range<usize>, source_len: usize) -> Range<usize> {
    let start = range.start.min(source_len);
    let end = range.end.min(source_len).max(start);
    start..end
}

fn import_statement_spans(source: &str) -> Vec<Range<usize>> {
    let bytes = source.as_bytes();
    let mut spans = Vec::new();
    let mut offset = 0;
    while offset < bytes.len() {
        let Some(relative) = source[offset..].find("import") else {
            break;
        };
        let start = offset + relative;
        let before_is_ident = start
            .checked_sub(1)
            .and_then(|index| bytes.get(index))
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_');
        let after = start + "import".len();
        let after_is_ident = bytes
            .get(after)
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_');
        if before_is_ident || after_is_ident {
            offset = after;
            continue;
        }
        let Some(end_relative) = source[after..].find(';') else {
            break;
        };
        let end = after + end_relative + 1;
        spans.push(start..end);
        offset = end;
    }
    spans
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf};

    use tempfile::tempdir;

    use super::AnalysisHost;

    #[test]
    fn parse_failure_keeps_last_good_ast() {
        let mut host = AnalysisHost::default();
        let path = PathBuf::from("shader.wesl");
        host.open(path.clone(), "fn good() {}".into());
        host.change(&path, "fn broken( {}".into());
        assert!(host.has_last_good_ast(&path));
        assert_eq!(host.diagnostics(&path).len(), 1);
    }

    #[test]
    fn unresolved_import_is_one_diagnostic() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("shader.wesl");
        fs::write(&path, "").unwrap();
        let mut host = AnalysisHost::default();
        host.open(
            path.clone(),
            "import package::missing::{value};\nfn main() { let x = value; }".into(),
        );
        let diagnostics = host.diagnostics(&path);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            &host.source(&path).unwrap()[diagnostics[0].range.clone()],
            "import package::missing::{value};"
        );
    }

    #[test]
    fn cyclic_import_is_reported_once() {
        let temp = tempdir().unwrap();
        let a = temp.path().join("a.wesl");
        let b = temp.path().join("b.wesl");
        let a_source = "import package::b::b_value;\nconst a_value = b_value;";
        let b_source = "import package::a::a_value;\nconst b_value = a_value;";
        fs::write(&a, a_source).unwrap();
        fs::write(&b, b_source).unwrap();
        let mut host = AnalysisHost::default();
        host.open(a.clone(), a_source.into());
        let diagnostics = host.diagnostics(&a);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("a -> b -> a"));
    }

    #[test]
    fn builtin_rename_is_rejected() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("shader.wesl");
        let source = "fn main() { let x = sin(1.0); }";
        fs::write(&path, source).unwrap();
        let mut host = AnalysisHost::default();
        host.open(path.clone(), source.into());
        let offset = source.find("sin").unwrap();
        assert_eq!(
            host.rename(&path, offset, "renamed"),
            Err("cannot rename a WGSL builtin")
        );
    }

    #[test]
    fn member_completions_use_last_good_types() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("shader.wesl");
        let good = "struct Item { value: f32, }\nfn main() { let v: vec3<f32> = vec3(0.0); let item: Item; let q = v; }";
        fs::write(&path, good).unwrap();
        let mut host = AnalysisHost::default();
        host.open(path.clone(), good.into());
        host.completions(&path, good.find("vec3(0.0)").unwrap());

        let vector_edit = good.replace("let q = v;", "v.");
        host.change(&path, vector_edit.clone());
        let vector_items = host.completions(&path, vector_edit.find("v.").unwrap() + 2);
        assert!(vector_items.iter().any(|item| item.label == "x"));
        assert!(vector_items.iter().any(|item| item.label == "xyz"));

        let struct_edit = good.replace("let q = v;", "item.");
        host.change(&path, struct_edit.clone());
        let struct_items = host.completions(&path, struct_edit.find("item.").unwrap() + 5);
        assert!(
            struct_items.iter().any(|item| item.label == "value"),
            "{struct_items:#?}"
        );
    }

    #[test]
    fn naga_oil_imports_navigate_without_type_errors() {
        let temp = tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let definition_path = root.join("mesh_functions.wgsl");
        let definition_source =
            "#define_import_path bevy_pbr::mesh_functions\nfn mesh_fn() -> f32 { return 1.0; }\n";
        fs::write(&definition_path, definition_source).unwrap();
        let path = root.join("main.wgsl");
        let source = "#import bevy_pbr::mesh_functions\n#ifdef FEATURE\nfn main() { let x: f32 = mesh_fn(); }\n#else\nfn main() { let x: bool = mesh_fn(); }\n#endif\n";
        fs::write(&path, source).unwrap();
        let mut host = AnalysisHost::new(Some(root));
        host.open(path.clone(), source.into());

        assert!(host.diagnostics(&path).is_empty());
        let import_offset = source.find("bevy_pbr").unwrap();
        let import_definition = host.definition(&path, import_offset).unwrap();
        assert_eq!(import_definition.path, definition_path);
        assert_eq!(
            &definition_source[import_definition.range],
            "bevy_pbr::mesh_functions"
        );
        let call_offset = source.find("mesh_fn").unwrap();
        assert_eq!(
            host.definition(&path, call_offset).unwrap().path,
            definition_path
        );
        assert!(
            host.completions(&path, call_offset)
                .iter()
                .any(|completion| completion.label == "mesh_fn")
        );
    }
}
