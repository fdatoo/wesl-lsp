use std::{
    collections::HashMap,
    ops::Range,
    path::{Path, PathBuf},
    sync::Arc,
};

use wgsl_parse::{parse_str, syntax::TranslationUnit};

use crate::{
    Completion, FoldingRange, HoverInfo, InlayHint, InlayHintConfig, Location, OverlayResolver,
    PackageIndex, SignatureHelp, SourceEdit, Symbol, WorkspaceSymbol, dialect, discover_root,
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
    roots: Vec<PathBuf>,
    documents: HashMap<PathBuf, Document>,
    packages: HashMap<PathBuf, PackageIndex>,
}

impl AnalysisHost {
    /// Accepts anything iterable, so a single `Option<PathBuf>` root and a multi-root
    /// `Vec<PathBuf>` both work.
    pub fn new(roots: impl IntoIterator<Item = PathBuf>) -> Self {
        Self {
            roots: canonical_roots(roots),
            documents: HashMap::new(),
            packages: HashMap::new(),
        }
    }

    pub fn set_roots(&mut self, roots: impl IntoIterator<Item = PathBuf>) {
        self.roots = canonical_roots(roots);
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

    /// A single configured root is authoritative — that is what an explicit root override
    /// means, and it holds even for a path that does not sit under it. With several roots
    /// there is nothing to override, so the innermost containing root wins and anything
    /// outside all of them falls back to discovery.
    pub fn root_for(&self, path: &Path) -> PathBuf {
        match self.roots.as_slice() {
            [] => discover_root(path, None),
            [only] => discover_root(path, Some(only)),
            roots => roots
                .iter()
                .filter(|root| path.starts_with(root))
                .max_by_key(|root| root.components().count())
                .cloned()
                .unwrap_or_else(|| discover_root(path, None)),
        }
    }

    pub fn has_last_good_ast(&self, path: &Path) -> bool {
        self.documents
            .get(path)
            .is_some_and(|document| document.last_good.is_some())
    }

    pub fn diagnostics(&mut self, path: &Path) -> Vec<Diagnostic> {
        let document = self.documents.get(path).cloned().or_else(|| {
            std::fs::read_to_string(path)
                .ok()
                .map(|source| Document::parse(source, None))
        });
        let Some(document) = document else {
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
            let type_diagnostics = self.ensure_package(path).type_diagnostics(path, module);
            diagnostics.extend(type_diagnostics.into_iter().map(|diagnostic| {
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

    pub fn diagnostic_batch(&mut self, path: &Path) -> Vec<(PathBuf, Vec<Diagnostic>)> {
        let paths = self.ensure_package(path).import_closure(path);
        paths
            .into_iter()
            .map(|path| {
                let diagnostics = self.diagnostics(&path);
                (path, diagnostics)
            })
            .collect()
    }

    pub fn rename(
        &mut self,
        path: &Path,
        offset: usize,
        new_name: &str,
    ) -> Result<Vec<SourceEdit>, &'static str> {
        self.ensure_package(path).rename(path, offset, new_name)
    }

    /// Searches the packages owning the currently open documents. Packages are built
    /// lazily, so this indexes the root of every open buffer first — the alternative
    /// would be scanning every root on disk, which the editor has not asked for.
    pub fn workspace_symbols(&mut self, query: &str) -> Vec<WorkspaceSymbol> {
        for path in self.documents.keys().cloned().collect::<Vec<_>>() {
            self.ensure_package(&path);
        }
        let mut symbols = self
            .packages
            .values()
            .flat_map(|package| package.workspace_symbols(query))
            .collect::<Vec<_>>();
        symbols.sort_by(|left, right| {
            left.symbol
                .path
                .cmp(&right.symbol.path)
                .then(left.symbol.range.start.cmp(&right.symbol.range.start))
        });
        symbols.dedup();
        symbols
    }

    /// Purely syntactic, so it works on buffers that do not parse.
    pub fn folding_ranges(&self, path: &Path) -> Vec<FoldingRange> {
        self.source(path)
            .map(crate::folding_ranges)
            .unwrap_or_default()
    }

    /// Innermost-first chain of ranges around `offset`, also purely syntactic.
    pub fn selection_ranges(&self, path: &Path, offset: usize) -> Vec<Range<usize>> {
        self.source(path)
            .map(|source| crate::selection_ranges(source, offset))
            .unwrap_or_default()
    }

    pub fn document_highlights(&mut self, path: &Path, offset: usize) -> Vec<Range<usize>> {
        self.ensure_package(path).document_highlights(path, offset)
    }

    pub fn prepare_rename(
        &mut self,
        path: &Path,
        offset: usize,
    ) -> Result<Range<usize>, &'static str> {
        self.ensure_package(path).prepare_rename(path, offset)
    }

    pub fn document_symbols(&mut self, path: &Path) -> Vec<Symbol> {
        self.ensure_package(path).document_symbols(path)
    }

    /// Import rewrites for a shader that is about to be renamed. Resolved against the old
    /// path, so this must be called before the file moves.
    pub fn file_rename_edits(&mut self, old_path: &Path, new_path: &Path) -> Vec<SourceEdit> {
        self.ensure_package(old_path)
            .file_rename_edits(old_path, new_path)
    }

    pub fn hover(&mut self, path: &Path, offset: usize) -> Option<HoverInfo> {
        self.ensure_package(path).hover(path, offset)
    }

    pub fn signature_help(&mut self, path: &Path, offset: usize) -> Option<SignatureHelp> {
        self.ensure_package(path).signature_help(path, offset)
    }

    pub fn inlay_hints(
        &mut self,
        path: &Path,
        range: Range<usize>,
        config: InlayHintConfig,
    ) -> Vec<InlayHint> {
        if config.is_empty() {
            return Vec::new();
        }
        self.ensure_package(path).inlay_hints(path, range, config)
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

fn canonical_roots(roots: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf> {
    let mut roots = roots
        .into_iter()
        .map(|root| root.canonicalize().unwrap_or(root))
        .collect::<Vec<_>>();
    roots.sort();
    roots.dedup();
    roots
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
    fn diagnostic_batch_covers_import_closure() {
        let temp = tempdir().unwrap();
        let dependency = temp.path().join("dependency.wesl");
        let dependency_source = "fn broken() { let value: bool = 1.0; }\n";
        fs::write(&dependency, dependency_source).unwrap();
        let root = temp.path().join("root.wesl");
        let root_source = "import package::dependency::broken;\nfn main() { broken(); }\n";
        fs::write(&root, root_source).unwrap();

        let mut host = AnalysisHost::default();
        host.open(root.clone(), root_source.into());
        let batch = host.diagnostic_batch(&root);

        assert_eq!(batch.len(), 2, "{batch:#?}");
        assert_eq!(batch[0], (root, Vec::new()));
        assert_eq!(batch[1].0, dependency);
        assert_eq!(batch[1].1.len(), 1);
        assert_eq!(
            batch[1].1[0].message,
            "type mismatch: expected bool, found f32"
        );
    }

    #[test]
    fn imported_function_and_struct_types_are_checked() {
        let temp = tempdir().unwrap();
        let types_path = temp.path().join("types.wesl");
        let types_source = "struct Item { value: f32, }\nfn make() -> Item { return Item(1.0); }\nfn scalar() -> f32 { return 1.0; }\n";
        fs::write(&types_path, types_source).unwrap();
        let path = temp.path().join("main.wesl");
        let source = "import package::types::{make, scalar as get_scalar, Item as ImportedItem};\nfn use_item(value: ImportedItem) { let x: f32 = value.value; }\nfn main() { let item = make(); let x: f32 = item.value; let bad: bool = get_scalar(); }\n";
        fs::write(&path, source).unwrap();
        let mut host = AnalysisHost::default();
        host.open(path.clone(), source.into());

        let diagnostics = host.diagnostics(&path);
        assert_eq!(diagnostics.len(), 1, "{diagnostics:#?}");
        assert_eq!(
            diagnostics[0].message,
            "type mismatch: expected bool, found f32"
        );

        let edited = source.replace("let bad: bool = get_scalar();", "make().");
        host.change(&path, edited.clone());
        let items = host.completions(&path, edited.rfind("make().").unwrap() + 7);
        assert!(items.iter().any(|item| item.label == "value"), "{items:#?}");
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
    fn renaming_a_shader_rewrites_importing_files() {
        let temp = tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let dependency = root.join("mesh.wesl");
        fs::write(&dependency, "const value = 1;\n").unwrap();
        // A same-prefixed neighbour that must not be rewritten.
        let neighbour = root.join("mesh_utils.wesl");
        fs::write(&neighbour, "const helper = 2;\n").unwrap();
        let importer = root.join("main.wesl");
        let source = concat!(
            "import package::mesh::value;\n",
            "import package::mesh_utils::helper;\n",
            "const total = value + helper;\n",
        );
        fs::write(&importer, source).unwrap();

        let mut host = AnalysisHost::new(Some(root.clone()));
        host.open(importer.clone(), source.into());

        let edits = host.file_rename_edits(&dependency, &root.join("geometry.wesl"));
        assert_eq!(edits.len(), 1, "{edits:#?}");
        assert_eq!(edits[0].path, importer);
        assert_eq!(&source[edits[0].range.clone()], "package::mesh");
        assert_eq!(edits[0].new_text, "package::geometry");

        // Applying the edit leaves the neighbour import untouched.
        let mut rewritten = source.to_owned();
        rewritten.replace_range(edits[0].range.clone(), &edits[0].new_text);
        assert!(rewritten.contains("import package::geometry::value;"));
        assert!(rewritten.contains("import package::mesh_utils::helper;"));

        // The boundary holds in the other direction too: renaming the longer-named neighbour
        // rewrites only its own import.
        let neighbour_edits = host.file_rename_edits(&neighbour, &root.join("helpers.wesl"));
        assert_eq!(neighbour_edits.len(), 1, "{neighbour_edits:#?}");
        assert_eq!(
            &source[neighbour_edits[0].range.clone()],
            "package::mesh_utils"
        );

        // A rename that does not change the module path is a no-op.
        assert!(host.file_rename_edits(&dependency, &dependency).is_empty());
    }

    #[test]
    fn renaming_a_directory_rewrites_outside_and_sibling_imports() {
        let temp = tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let mesh = root.join("mesh");
        fs::create_dir(&mesh).unwrap();

        // Two shaders inside the directory, one importing the other.
        fs::write(mesh.join("common.wesl"), "const scale = 2.0;\n").unwrap();
        let sibling_source = "import package::mesh::common::scale;\nconst doubled = scale * 2.0;\n";
        fs::write(mesh.join("surface.wesl"), sibling_source).unwrap();

        // And one outside importing in.
        let outside = root.join("main.wesl");
        let outside_source = "import package::mesh::surface::doubled;\nconst total = doubled;\n";
        fs::write(&outside, outside_source).unwrap();

        let mut host = AnalysisHost::new(Some(root.clone()));
        host.open(outside.clone(), outside_source.into());

        let edits = host.file_rename_edits(&mesh, &root.join("geometry"));

        // The external importer is rewritten.
        let external = edits
            .iter()
            .find(|edit| edit.path == outside)
            .unwrap_or_else(|| panic!("{edits:#?}"));
        assert_eq!(
            &outside_source[external.range.clone()],
            "package::mesh::surface"
        );
        assert_eq!(external.new_text, "package::geometry::surface");

        // And so is the sibling inside the moved directory — the case a path-based skip
        // would silently miss.
        let sibling = edits
            .iter()
            .find(|edit| edit.path == mesh.join("surface.wesl"))
            .unwrap_or_else(|| panic!("sibling import not rewritten: {edits:#?}"));
        assert_eq!(
            &sibling_source[sibling.range.clone()],
            "package::mesh::common"
        );
        assert_eq!(sibling.new_text, "package::geometry::common");

        // Renaming a directory nothing imports from produces nothing.
        let unrelated = root.join("unused");
        fs::create_dir(&unrelated).unwrap();
        assert!(
            host.file_rename_edits(&unrelated, &root.join("other"))
                .is_empty()
        );
    }

    #[test]
    fn each_workspace_folder_resolves_against_its_own_root() {
        let temp = tempdir().unwrap();
        let base = temp.path().canonicalize().unwrap();
        let first = base.join("first");
        let second = base.join("second");
        fs::create_dir_all(first.join("nested")).unwrap();
        fs::create_dir(&second).unwrap();

        // Same module name in both roots, resolving to different files.
        fs::write(first.join("shared.wesl"), "const value: f32 = 1.0;\n").unwrap();
        fs::write(second.join("shared.wesl"), "const value: f32 = 2.0;\n").unwrap();

        let first_main = first.join("main.wesl");
        let first_source = "import package::shared::value;\nconst total: f32 = value;\n";
        fs::write(&first_main, first_source).unwrap();
        let second_main = second.join("main.wesl");
        let second_source = "import package::shared::value;\nconst total: f32 = value;\n";
        fs::write(&second_main, second_source).unwrap();

        let mut host = AnalysisHost::new(vec![first.clone(), second.clone()]);
        assert_eq!(host.root_for(&first_main), first);
        assert_eq!(host.root_for(&second_main), second);

        // Both resolve cleanly, each against its own package.
        host.open(first_main.clone(), first_source.into());
        host.open(second_main.clone(), second_source.into());
        assert!(host.diagnostics(&first_main).is_empty());
        assert!(host.diagnostics(&second_main).is_empty());

        // The innermost containing root wins when roots nest.
        let mut nested = AnalysisHost::new(vec![base.clone(), first.clone()]);
        assert_eq!(nested.root_for(&first_main), first);
        assert_eq!(nested.root_for(&second_main), base);

        // A path outside every root falls back to discovery rather than an arbitrary root.
        let outside = base.join("loose.wesl");
        fs::write(&outside, "const value = 1;\n").unwrap();
        let elsewhere = AnalysisHost::new(vec![first.clone(), second.clone()]);
        assert_eq!(
            elsewhere.root_for(&outside),
            crate::discover_root(&outside, None)
        );

        // Dropping a folder reindexes: only the surviving root is used.
        nested.set_roots(vec![base.clone()]);
        assert_eq!(nested.root_for(&first_main), base);
    }

    #[test]
    fn prepare_rename_agrees_with_rename() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("shader.wesl");
        let source = "fn scale(factor: f32) -> f32 { return factor * sin(1.0); }";
        fs::write(&path, source).unwrap();
        let mut host = AnalysisHost::default();
        host.open(path.clone(), source.into());

        let parameter = source.find("factor").unwrap();
        assert_eq!(
            host.prepare_rename(&path, parameter),
            Ok(parameter..parameter + "factor".len())
        );

        let builtin = source.find("sin").unwrap();
        assert_eq!(
            host.prepare_rename(&path, builtin),
            Err("cannot rename a WGSL builtin")
        );
        assert!(host.rename(&path, builtin, "renamed").is_err());

        let literal = source.find("1.0").unwrap();
        assert_eq!(
            host.prepare_rename(&path, literal),
            Err("no symbol to rename here")
        );
    }

    #[test]
    fn workspace_symbols_span_files_and_include_struct_members() {
        let temp = tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let types_path = root.join("types.wesl");
        fs::write(&types_path, "struct Camera { projection: mat4x4<f32>, }\n").unwrap();
        let main_path = root.join("main.wesl");
        let main_source = "fn project() -> f32 { return 1.0; }\n";
        fs::write(&main_path, main_source).unwrap();

        let mut host = AnalysisHost::new(Some(root));
        host.open(main_path.clone(), main_source.into());

        // Matching is case-insensitive and substring-based, so "proj" spans both files.
        let found = host.workspace_symbols("proj");
        let names = found
            .iter()
            .map(|found| found.symbol.name.as_str())
            .collect::<Vec<_>>();
        assert!(names.contains(&"projection"), "{names:?}");
        assert!(names.contains(&"project"), "{names:?}");

        let member = found
            .iter()
            .find(|found| found.symbol.name == "projection")
            .unwrap();
        assert_eq!(member.container.as_deref(), Some("Camera"));
        assert_eq!(member.symbol.path, types_path);

        assert!(host.workspace_symbols("nothing_matches_this").is_empty());
        assert!(host.workspace_symbols("").len() >= 3);
    }

    /// Layout hints are off by default, so tests that want every kind opt in explicitly.
    fn all_hints() -> crate::InlayHintConfig {
        crate::InlayHintConfig {
            struct_layout_hints: true,
            ..crate::InlayHintConfig::default()
        }
    }

    #[test]
    fn inlay_hint_config_gates_each_kind() {
        use crate::{InlayHintConfig, InlayKind};

        let temp = tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let path = root.join("main.wesl");
        let source = concat!(
            "struct Camera { origin: vec3<f32>, focal: f32, }\n",
            "fn shade(albedo: f32, roughness: f32) -> f32 { return albedo * roughness; }\n",
            "fn main() { let lit = shade(1.0, 0.5); }\n",
        );
        fs::write(&path, source).unwrap();
        let mut host = AnalysisHost::new(Some(root));
        host.open(path.clone(), source.into());

        let mut kinds = |config| {
            host.inlay_hints(&path, 0..source.len(), config)
                .into_iter()
                .map(|hint| hint.kind)
                .collect::<Vec<_>>()
        };

        // The default must not include layout hints — they are opt-in.
        let default_kinds = kinds(InlayHintConfig::default());
        assert!(
            !default_kinds.contains(&InlayKind::Layout),
            "layout hints must be off by default: {default_kinds:?}"
        );
        assert!(
            default_kinds.contains(&InlayKind::Type),
            "{default_kinds:?}"
        );
        assert!(
            default_kinds.contains(&InlayKind::Parameter),
            "{default_kinds:?}"
        );

        // Opting in adds them without disturbing the others.
        assert!(kinds(all_hints()).contains(&InlayKind::Layout));

        // Each kind gates independently.
        let only_layout = kinds(InlayHintConfig {
            type_hints: false,
            parameter_hints: false,
            struct_layout_hints: true,
        });
        assert!(
            only_layout.iter().all(|kind| *kind == InlayKind::Layout),
            "{only_layout:?}"
        );

        let nothing = InlayHintConfig {
            type_hints: false,
            parameter_hints: false,
            struct_layout_hints: false,
        };
        assert!(nothing.is_empty());
        assert!(kinds(nothing).is_empty());
    }

    #[test]
    fn inlay_hints_show_inferred_types_and_parameter_names() {
        use crate::InlayKind;

        let temp = tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let path = root.join("main.wesl");
        let source = concat!(
            "fn shade(albedo: f32, roughness: f32) -> f32 { return albedo * roughness; }\n",
            "fn main() {\n",
            "    let tint = 0.5;\n",
            "    let annotated: f32 = 1.0;\n",
            "    let lit = shade(tint, 0.25);\n",
            "}\n",
        );
        fs::write(&path, source).unwrap();
        let mut host = AnalysisHost::new(Some(root));
        host.open(path.clone(), source.into());

        let hints = host.inlay_hints(&path, 0..source.len(), all_hints());
        let rendered = hints
            .iter()
            .map(|hint| (hint.kind, hint.label.as_str()))
            .collect::<Vec<_>>();

        assert!(
            rendered.contains(&(InlayKind::Type, ": f32")),
            "inferred let should be hinted: {rendered:#?}"
        );
        // An explicitly annotated declaration must not be hinted.
        assert_eq!(
            rendered
                .iter()
                .filter(|(kind, _)| *kind == InlayKind::Type)
                .count(),
            2,
            "only `tint` and `lit` lack annotations: {rendered:#?}"
        );
        // `tint` is passed to the `albedo` parameter, `0.25` to `roughness`.
        assert!(
            rendered.contains(&(InlayKind::Parameter, "albedo:")),
            "{rendered:#?}"
        );
        assert!(
            rendered.contains(&(InlayKind::Parameter, "roughness:")),
            "{rendered:#?}"
        );

        // Hints are anchored where the editor should draw them.
        let type_hint = hints
            .iter()
            .find(|hint| hint.kind == InlayKind::Type)
            .unwrap();
        assert_eq!(
            &source[..type_hint.offset],
            &source[..source.find("tint").unwrap() + "tint".len()]
        );

        // Requesting a sub-range returns only the hints inside it.
        let second_line = source.find("let lit").unwrap();
        let narrowed = host.inlay_hints(&path, second_line..source.len(), all_hints());
        assert!(narrowed.iter().all(|hint| hint.offset >= second_line));
        assert!(!narrowed.is_empty());
    }

    #[test]
    fn struct_layout_hints_expose_uniform_padding() {
        use crate::InlayKind;

        let temp = tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let path = root.join("main.wesl");
        // `radius` before `position` forces 12 bytes of padding — the classic uniform bug.
        let source = concat!(
            "struct Sphere {\n",
            "    radius: f32,\n",
            "    position: vec3<f32>,\n",
            "}\n",
            "@group(0) @binding(0) var<uniform> sphere: Sphere;\n",
            "fn main() { let r = sphere.radius; }\n",
        );
        fs::write(&path, source).unwrap();
        let mut host = AnalysisHost::new(Some(root));
        host.open(path.clone(), source.into());

        let layout = host
            .inlay_hints(&path, 0..source.len(), all_hints())
            .into_iter()
            .filter(|hint| hint.kind == InlayKind::Layout)
            .map(|hint| hint.label)
            .collect::<Vec<_>>();
        assert_eq!(
            layout,
            vec![
                "offset 0, align 4, size 4".to_owned(),
                "offset 16, align 16, size 12".to_owned(),
            ],
            "vec3 must be pushed to offset 16"
        );
    }

    #[test]
    fn parameter_hints_are_suppressed_when_the_name_already_matches() {
        use crate::InlayKind;

        let temp = tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let path = root.join("main.wesl");
        let source = concat!(
            "fn shade(albedo: f32, roughness: f32) -> f32 { return albedo * roughness; }\n",
            "fn main(albedo: f32) { let x = shade(albedo, 0.5); }\n",
        );
        fs::write(&path, source).unwrap();
        let mut host = AnalysisHost::new(Some(root));
        host.open(path.clone(), source.into());

        let labels = host
            .inlay_hints(&path, 0..source.len(), all_hints())
            .into_iter()
            .filter(|hint| hint.kind == InlayKind::Parameter)
            .map(|hint| hint.label)
            .collect::<Vec<_>>();
        assert_eq!(labels, vec!["roughness:".to_owned()], "{labels:#?}");
    }

    #[test]
    fn member_completions_use_last_good_types() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("shader.wesl");
        let good = "struct Item { value: f32, }\nfn make() -> Item { return Item(1.0); }\nfn main() { let v: vec3<f32> = vec3(0.0); var item: Item; let q = v; }";
        fs::write(&path, good).unwrap();
        let mut host = AnalysisHost::default();
        host.open(path.clone(), good.into());
        host.completions(&path, good.find("vec3(0.0)").unwrap());

        let vector_edit = good.replace("let q = v;", "v.");
        host.change(&path, vector_edit.clone());
        let vector_items = host.completions(&path, vector_edit.find("v.").unwrap() + 2);
        assert!(vector_items.iter().any(|item| item.label == "x"));
        assert!(vector_items.iter().any(|item| item.label == "xyz"));

        let binary_edit = good.replace("let q = v;", "(v + v).");
        host.change(&path, binary_edit.clone());
        let binary_items = host.completions(&path, binary_edit.find("(v + v).").unwrap() + 8);
        assert!(binary_items.iter().any(|item| item.label == "zyxx"));

        let struct_edit = good.replace("let q = v;", "item.");
        host.change(&path, struct_edit.clone());
        let struct_items = host.completions(&path, struct_edit.find("item.").unwrap() + 5);
        assert!(
            struct_items.iter().any(|item| item.label == "value"),
            "{struct_items:#?}"
        );

        let call_edit = good.replace("let q = v;", "make().");
        host.change(&path, call_edit.clone());
        let call_items = host.completions(&path, call_edit.find("make().").unwrap() + 7);
        assert!(call_items.iter().any(|item| item.label == "value"));
    }

    #[test]
    fn nested_shadow_does_not_escape_its_block() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("shader.wesl");
        let source = "fn f() { let x = 1; { let x = 2; let y = x; } let z = x; }";
        fs::write(&path, source).unwrap();
        let mut host = AnalysisHost::default();
        host.open(path.clone(), source.into());

        let final_use = source.rfind("x;").unwrap();
        let definition = host.definition(&path, final_use).unwrap();
        assert_eq!(definition.range.start, source.find("x = 1").unwrap());
    }

    #[test]
    fn branch_local_does_not_escape_to_sibling() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("shader.wesl");
        let source =
            "fn f(condition: bool) { if condition { let branch = 1; } else { let x = branch; } }";
        fs::write(&path, source).unwrap();
        let mut host = AnalysisHost::default();
        host.open(path.clone(), source.into());

        let escaped_use = source.rfind("branch;").unwrap();
        assert!(host.definition(&path, escaped_use).is_none());
    }

    #[test]
    fn for_initializer_does_not_escape_loop() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("shader.wesl");
        let source = "fn f() { for (var i = 0; i < 2; i++) {} let x = i; }";
        fs::write(&path, source).unwrap();
        let mut host = AnalysisHost::default();
        host.open(path.clone(), source.into());

        let escaped_use = source.rfind("i;").unwrap();
        assert!(host.definition(&path, escaped_use).is_none());
    }

    #[test]
    fn struct_members_have_definitions_and_references() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("shader.wesl");
        let source = "struct Output { value: f32, }\nfn f(out: Output) { let copy = out.value; }\n";
        fs::write(&path, source).unwrap();
        let mut host = AnalysisHost::default();
        host.open(path.clone(), source.into());

        let declaration = source.find("value:").unwrap();
        let usage = source.rfind("value").unwrap();
        let completions = host.completions(&path, usage);
        assert!(
            completions
                .iter()
                .any(|completion| completion.label == "value"),
            "{completions:#?}"
        );
        let symbols = host.document_symbols(&path);
        let definition = host.definition(&path, usage);
        assert!(definition.is_some(), "{symbols:#?}");
        assert_eq!(definition.unwrap().range.start, declaration);
        let references = host.references(&path, declaration, true);
        assert_eq!(references.len(), 2, "{references:#?}");
        assert!(
            references
                .iter()
                .any(|location| location.range.start == usage)
        );
    }

    #[test]
    fn valid_vertex_struct_members_have_no_diagnostics() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("shader.wesl");
        let source = r#"
struct Globals {
    screen: vec2<f32>,
    _pad: vec2<f32>,
}
@group(0) @binding(0) var<uniform> globals: Globals;
struct VsIn {
    @location(0) pos: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
}
struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
}
@vertex
fn vs_main(in: VsIn) -> VsOut {
    var out: VsOut;
    let ndc = vec2<f32>(
        in.pos.x / globals.screen.x * 2.0 - 1.0,
        1.0 - in.pos.y / globals.screen.y * 2.0,
    );
    out.clip = vec4<f32>(ndc, 0.0, 1.0);
    out.uv = in.uv;
    out.color = in.color;
    return out;
}
"#;
        fs::write(&path, source).unwrap();
        let mut host = AnalysisHost::new(Some(temp.path().to_path_buf()));
        host.open(path.clone(), source.into());
        assert!(host.diagnostics(&path).is_empty());

        let broken = source.replace("out.uv = in.uv", "out.out = in.uv");
        host.change(&path, broken.clone());
        let diagnostics = host.diagnostics(&path);
        assert_eq!(diagnostics.len(), 1, "{diagnostics:#?}");
        assert_eq!(diagnostics[0].message, "type has no member out");
        let member = broken.find(".out =").unwrap() + 1;
        assert_eq!(diagnostics[0].range, member..member + 3);

        let unknown = source.replace("out.uv = in.uv", "asdf.sdf++;\n    out.uv = in.uv");
        host.change(&path, unknown.clone());
        let diagnostics = host.diagnostics(&path);
        assert_eq!(diagnostics.len(), 1, "{diagnostics:#?}");
        assert_eq!(diagnostics[0].message, "unresolved identifier asdf");
        let identifier = unknown.find("asdf").unwrap();
        assert_eq!(diagnostics[0].range, identifier..identifier + 4);

        host.change(&path, source.into());
        let diagnostics = host.diagnostics(&path);
        assert!(diagnostics.is_empty(), "{diagnostics:#?}");
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
