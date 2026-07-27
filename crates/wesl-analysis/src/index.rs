use std::{
    collections::{HashMap, HashSet},
    fs,
    ops::Range,
    path::{Path, PathBuf},
    sync::Arc,
};

use crate::{
    builtins::{BUILTIN_FUNCTIONS, BUILTIN_TYPES, builtin},
    dialect,
    inlay::{InlayHint, InlayHintConfig, InlayKind},
    layout::{MemberOverrides, struct_layout},
    signature::{
        CallSite, SignatureHelp, SignatureInfo, call_sites, enclosing_call, parameter_names,
        parameter_spans,
    },
    ty::{
        Ty, TypeDiagnostic, TypeEnvironment, analyze_module, collect_struct_types, const_u32,
        infer_expression_type, inferred_declarations,
    },
};
use smol_str::SmolStr;
use walkdir::WalkDir;
use wgsl_parse::{
    SyntaxNode, parse_str,
    syntax::{
        Attribute, CompoundStatement, Expression, GlobalDeclaration, ImportContent, ImportItem,
        ModulePath, PathOrigin, Statement, StatementNode, TranslationUnit,
    },
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SymbolKind {
    Function,
    Struct,
    Field,
    Variable,
    Constant,
    Override,
    Alias,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Symbol {
    pub name: SmolStr,
    pub kind: SymbolKind,
    pub path: PathBuf,
    pub range: Range<usize>,
    pub full_range: Range<usize>,
    pub signature: String,
    pub documentation: Option<String>,
    pub children: Vec<Symbol>,
}

/// A [`Symbol`] flattened out of its file, for `workspace/symbol`. Struct members keep
/// the name of the struct they came from so the editor can disambiguate same-named fields.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceSymbol {
    pub symbol: Symbol,
    pub container: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Location {
    pub path: PathBuf,
    pub range: Range<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceEdit {
    pub path: PathBuf,
    pub range: Range<usize>,
    pub new_text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HoverInfo {
    pub signature: String,
    pub documentation: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompletionKind {
    Function,
    Struct,
    Field,
    Variable,
    Type,
    Keyword,
    Snippet,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Completion {
    pub label: String,
    pub kind: CompletionKind,
    pub detail: Option<String>,
    pub insert_text: Option<String>,
    pub additional_edit: Option<SourceEdit>,
}

#[derive(Clone, Debug)]
struct ImportBinding {
    local_name: SmolStr,
    original_name: SmolStr,
    module: ModulePath,
}

#[derive(Clone, Debug)]
struct FileIndex {
    source: Arc<str>,
    module: Option<Arc<TranslationUnit>>,
    symbols: Vec<Symbol>,
    locals: Vec<LocalSymbol>,
    imports: Vec<ImportBinding>,
    imported_modules: Vec<ModulePath>,
    oil_imports: Vec<(String, Range<usize>)>,
    oil_definitions: Vec<(String, Range<usize>)>,
    /// A naga_oil file. `dialect::preprocess` blanks the directives but keeps every branch,
    /// so the parsed module merges the bodies of `#ifdef` and `#else` into one contradictory
    /// picture. Navigation still works over it; type-level conclusions do not.
    dialect: bool,
    /// Set when [`PackageIndex::update`] keeps this file's last successfully parsed
    /// `symbols`/`locals`/`imports` because the new text failed to reparse. Those
    /// collections still hold byte ranges measured against the OLD text even though
    /// `source` has already moved to the new one, so any position drawn from them is
    /// only trustworthy until the next successful reindex clears this flag. Names and
    /// types stay usable regardless -- only positions are suspect.
    positions_stale: bool,
}

#[derive(Clone, Debug)]
struct LocalSymbol {
    name: SmolStr,
    range: Range<usize>,
    scope_range: Range<usize>,
}

#[derive(Clone, Debug)]
pub(crate) struct PackageIndex {
    root: PathBuf,
    files: HashMap<PathBuf, FileIndex>,
}

impl PackageIndex {
    pub(crate) fn build(root: PathBuf, overlays: &HashMap<PathBuf, Arc<str>>) -> Self {
        let mut files = HashMap::new();
        for entry in WalkDir::new(&root).into_iter().filter_map(Result::ok) {
            if !entry.file_type().is_file() || !is_shader(entry.path()) {
                continue;
            }
            let path = entry.path().to_path_buf();
            let source = overlays
                .get(&path)
                .cloned()
                .or_else(|| fs::read_to_string(&path).ok().map(Arc::<str>::from));
            if let Some(source) = source {
                files.insert(path.clone(), FileIndex::new(path, source));
            }
        }
        for (path, source) in overlays {
            if path.starts_with(&root) && is_shader(path) && !files.contains_key(path) {
                files.insert(path.clone(), FileIndex::new(path.clone(), source.clone()));
            }
        }
        Self { root, files }
    }

    pub(crate) fn update(&mut self, path: PathBuf, source: Arc<str>) {
        if parse_str(&dialect::preprocess(&source)).is_err()
            && let Some(existing) = self.files.get_mut(&path)
        {
            // The AST-derived `symbols`/`locals`/`imports` cannot be refreshed without a
            // successful parse, so they are kept and flagged stale (see
            // `FileIndex::positions_stale`) instead of dropped -- callers that only need
            // names or types still get a useful last-good answer. `oil_imports` /
            // `oil_definitions` / `dialect` are plain text scans with no parse
            // dependency, so there is no reason to let them go stale too.
            existing.oil_imports = dialect::imports(&source);
            existing.oil_definitions = dialect::definitions(&source);
            existing.dialect = dialect::is_naga_oil(&source);
            existing.positions_stale = true;
            existing.source = source;
            return;
        }
        self.files
            .insert(path.clone(), FileIndex::new(path, source));
    }

    pub(crate) fn definition(&self, path: &Path, offset: usize) -> Option<Location> {
        if let Some(file) = self.files.get(path)
            && let Some((import, _)) = file
                .oil_imports
                .iter()
                .find(|(_, range)| range.start <= offset && offset <= range.end)
            && let Some((target_path, target_range)) = self.oil_definition(import)
        {
            return Some(Location {
                path: target_path.clone(),
                range: target_range.clone(),
            });
        }
        let name = identifier_at(self.files.get(path)?.source.as_ref(), offset)?;
        self.resolve(path, &name, offset).and_then(|symbol| {
            let range = self.current_range(&symbol)?;
            Some(Location {
                path: symbol.path,
                range,
            })
        })
    }

    pub(crate) fn references(
        &self,
        path: &Path,
        offset: usize,
        include_declaration: bool,
    ) -> Vec<Location> {
        let Some(file) = self.files.get(path) else {
            return Vec::new();
        };
        let Some(name) = identifier_at(&file.source, offset) else {
            return Vec::new();
        };
        let Some(target) = self.resolve(path, &name, offset) else {
            return Vec::new();
        };
        let target_key = (target.path.clone(), target.range.clone());
        let declaration_range = self.current_range(&target);
        let mut locations = Vec::new();
        if include_declaration && let Some(range) = declaration_range.clone() {
            locations.push(Location {
                path: target.path.clone(),
                range,
            });
        }
        for (candidate_path, candidate) in &self.files {
            for range in identifier_ranges(&candidate.source, &name) {
                if candidate_path == &target.path && declaration_range.as_ref() == Some(&range) {
                    continue;
                }
                if self
                    .resolve(candidate_path, &name, range.start)
                    .is_some_and(|symbol| (symbol.path.clone(), symbol.range.clone()) == target_key)
                {
                    locations.push(Location {
                        path: candidate_path.clone(),
                        range,
                    });
                }
            }
        }
        locations.sort_by(|left, right| {
            left.path
                .cmp(&right.path)
                .then(left.range.start.cmp(&right.range.start))
        });
        locations.dedup();
        locations
    }

    pub(crate) fn rename(
        &self,
        path: &Path,
        offset: usize,
        new_name: &str,
    ) -> Result<Vec<SourceEdit>, &'static str> {
        if !is_identifier(new_name) {
            return Err("invalid WGSL identifier");
        }
        let Some(file) = self.files.get(path) else {
            return Ok(Vec::new());
        };
        let Some(name) = identifier_at(&file.source, offset) else {
            return Ok(Vec::new());
        };
        if is_builtin(&name) {
            return Err("cannot rename a WGSL builtin");
        }
        Ok(self
            .references(path, offset, true)
            .into_iter()
            .map(|location| SourceEdit {
                path: location.path,
                range: location.range,
                new_text: new_name.to_owned(),
            })
            .collect())
    }

    /// Occurrences within this file only.
    ///
    /// Deliberately not `references(..).filter(same file)`: editors ask for highlights on
    /// every cursor movement, and `references` scans every file in the package, so filtering
    /// afterwards would throw away a whole-workspace walk on each keystroke.
    pub(crate) fn document_highlights(&self, path: &Path, offset: usize) -> Vec<Range<usize>> {
        let Some(file) = self.files.get(path) else {
            return Vec::new();
        };
        let Some(name) = identifier_at(&file.source, offset) else {
            return Vec::new();
        };
        let Some(target) = self.resolve(path, &name, offset) else {
            return Vec::new();
        };
        let target_key = (target.path.clone(), target.range.clone());
        let declaration_range = (target.path == path)
            .then(|| self.current_range(&target))
            .flatten();

        let mut ranges = Vec::new();
        if let Some(range) = declaration_range.clone() {
            ranges.push(range);
        }
        for range in identifier_ranges(&file.source, &name) {
            if declaration_range.as_ref() == Some(&range) {
                continue;
            }
            if self
                .resolve(path, &name, range.start)
                .is_some_and(|symbol| (symbol.path, symbol.range) == target_key)
            {
                ranges.push(range);
            }
        }
        ranges.sort_by_key(|range| range.start);
        ranges.dedup();
        ranges
    }

    /// Validates that `offset` sits on something renameable and returns the range the
    /// editor should pre-fill. Rejects the same cases [`Self::rename`] would, so the
    /// editor reports them before the user types a new name rather than after.
    pub(crate) fn prepare_rename(
        &self,
        path: &Path,
        offset: usize,
    ) -> Result<Range<usize>, &'static str> {
        let file = self.files.get(path).ok_or("no symbol to rename here")?;
        let (name, range) =
            identifier_range_at(&file.source, offset).ok_or("no symbol to rename here")?;
        if is_builtin(&name) {
            return Err("cannot rename a WGSL builtin");
        }
        if self.resolve(path, &name, offset).is_none() {
            return Err("no symbol to rename here");
        }
        Ok(range)
    }

    /// Edits that keep imports pointing at a shader that is about to be renamed. Called
    /// before the rename happens, so the index still describes the old layout.
    /// Import rewrites for a rename. `old_path` may be a single shader or a directory, in
    /// which case every shader beneath it moves and each contributes its own rewrite.
    pub(crate) fn file_rename_edits(&self, old_path: &Path, new_path: &Path) -> Vec<SourceEdit> {
        // Shaders whose module path changes, paired with where they land.
        let moved = if self.files.contains_key(old_path) {
            vec![(old_path.to_path_buf(), new_path.to_path_buf())]
        } else {
            self.files
                .keys()
                .filter_map(|path| {
                    let relative = path.strip_prefix(old_path).ok()?;
                    Some((path.clone(), new_path.join(relative)))
                })
                .collect()
        };

        let rewrites = moved
            .iter()
            .filter_map(|(old_file, new_file)| {
                let old_module = module_name(&self.root, old_file)?;
                let new_module = module_name(&self.root, new_file)?;
                (old_module != new_module).then(|| {
                    (
                        old_file.clone(),
                        format!("package::{old_module}"),
                        format!("package::{new_module}"),
                    )
                })
            })
            .collect::<Vec<_>>();
        if rewrites.is_empty() {
            return Vec::new();
        }

        let mut edits = Vec::new();
        for (path, file) in &self.files {
            // Nothing is skipped by path: when a directory moves, a shader inside it that
            // imports a sibling in that same directory still needs rewriting.
            for (target, old_text, new_text) in &rewrites {
                // Resolve first: only rewrite text in files that genuinely import this
                // module, so a same-named module in another package is never touched.
                let imports_target = file
                    .imports
                    .iter()
                    .map(|binding| &binding.module)
                    .chain(file.imported_modules.iter())
                    .any(|module| self.module_file(module).as_deref() == Some(target.as_path()));
                if !imports_target {
                    continue;
                }
                edits.extend(
                    import_path_ranges(&file.source, old_text)
                        .into_iter()
                        .map(|range| SourceEdit {
                            path: path.clone(),
                            range,
                            new_text: new_text.clone(),
                        }),
                );
            }
        }
        edits.sort_by(|left, right| {
            left.path
                .cmp(&right.path)
                .then(left.range.start.cmp(&right.range.start))
        });
        edits.dedup();
        edits
    }

    pub(crate) fn document_symbols(&self, path: &Path) -> Vec<Symbol> {
        self.files
            .get(path)
            .filter(|file| !file.positions_stale)
            .map(|file| file.symbols.clone())
            .unwrap_or_default()
    }

    /// Hints whose anchor falls inside `range`. Type hints need the module to have parsed;
    /// parameter hints are token-based and survive a broken buffer.
    pub(crate) fn inlay_hints(
        &self,
        path: &Path,
        range: Range<usize>,
        config: InlayHintConfig,
    ) -> Vec<InlayHint> {
        let Some(file) = self.files.get(path) else {
            return Vec::new();
        };
        let mut hints = Vec::new();

        // Type and layout hints are type-level conclusions, so they are unsound on a merged
        // dialect module for the same reason `AnalysisHost::diagnostics` skips it. They also
        // read spans straight out of `module`, which go stale exactly like `Symbol`/
        // `LocalSymbol` ranges (see `FileIndex::positions_stale`) once a reparse fails --
        // skip here too, or `find_identifier` below slices the CURRENT (possibly shorter)
        // `file.source` with a byte range measured against the OLD one and panics. Parameter
        // hints are token-based against the current source and stay, as does navigation.
        if let Some(module) = file.module.as_deref()
            && !file.positions_stale
            && !file.dialect
            && (config.type_hints || config.struct_layout_hints)
        {
            let mut active = HashSet::from([path.to_path_buf()]);
            let imports = self.imported_type_environment(file, &mut active);
            if config.struct_layout_hints {
                hints.extend(struct_layout_hints(module, imports.clone(), &range));
            }
            if config.type_hints {
                for (declaration, name, ty) in inferred_declarations(module, imports) {
                    if let Some(identifier) = find_identifier(&file.source, declaration, &name)
                        && range.contains(&identifier.end)
                    {
                        hints.push(InlayHint {
                            offset: identifier.end,
                            label: format!(": {ty}"),
                            kind: InlayKind::Type,
                        });
                    }
                }
            }
        }

        if config.parameter_hints {
            for call in call_sites(&file.source) {
                let Some(label) = self.callee_label(path, &call) else {
                    continue;
                };
                let names = parameter_names(&label);
                for (index, argument) in call.arguments.iter().enumerate() {
                    let Some(name) = names.get(index).filter(|name| !name.is_empty()) else {
                        break;
                    };
                    // Labelling `shade(albedo)` with `albedo:` is pure noise.
                    if !range.contains(argument)
                        || identifier_at(&file.source, *argument).as_deref() == Some(name.as_str())
                    {
                        continue;
                    }
                    hints.push(InlayHint {
                        offset: *argument,
                        label: format!("{name}:"),
                        kind: InlayKind::Parameter,
                    });
                }
            }
        }

        hints.sort_by_key(|hint| hint.offset);
        hints
    }

    /// The signature label to take parameter names from, for a user function, a struct used
    /// as a constructor, or a builtin overload matching the argument count.
    fn callee_label(&self, path: &Path, call: &CallSite) -> Option<String> {
        if let Some(symbol) = self.resolve(path, &call.callee, call.callee_range.start) {
            return match symbol.kind {
                SymbolKind::Function => Some(symbol.signature.clone()),
                SymbolKind::Struct => Some(struct_constructor_label(&symbol)),
                _ => None,
            };
        }
        let builtin = builtin(&call.callee)?;
        builtin
            .overloads
            .iter()
            .find(|overload| parameter_spans(overload.signature).len() == call.arguments.len())
            .or_else(|| builtin.overloads.first())
            .map(|overload| overload.signature.to_owned())
    }

    pub(crate) fn signature_help(&self, path: &Path, offset: usize) -> Option<SignatureHelp> {
        let file = self.files.get(path)?;
        let (callee, active_parameter) = enclosing_call(&file.source, offset)?;

        if let Some(symbol) = self.resolve(path, &callee, offset) {
            let label = match symbol.kind {
                SymbolKind::Function => symbol.signature.clone(),
                SymbolKind::Struct => struct_constructor_label(&symbol),
                _ => return None,
            };
            return Some(SignatureHelp {
                signatures: vec![SignatureInfo {
                    parameters: parameter_spans(&label),
                    label,
                    documentation: symbol.documentation.clone(),
                }],
                active_signature: 0,
                active_parameter,
            });
        }

        let signatures = builtin(&callee)?
            .overloads
            .iter()
            .map(|overload| SignatureInfo {
                label: overload.signature.to_owned(),
                parameters: parameter_spans(overload.signature),
                documentation: (!overload.doc.is_empty()).then(|| overload.doc.to_owned()),
            })
            .collect::<Vec<_>>();
        // Prefer an overload that actually has the argument the cursor is sitting on.
        let active_signature = signatures
            .iter()
            .position(|signature| signature.parameters.len() > active_parameter)
            .unwrap_or(0);
        Some(SignatureHelp {
            signatures,
            active_signature,
            active_parameter,
        })
    }

    pub(crate) fn workspace_symbols(&self, query: &str) -> Vec<WorkspaceSymbol> {
        let query = query.to_lowercase();
        let mut matches = Vec::new();
        for file in self.files.values().filter(|file| !file.positions_stale) {
            for symbol in &file.symbols {
                if name_matches(&symbol.name, &query) {
                    matches.push(WorkspaceSymbol {
                        symbol: symbol.clone(),
                        container: None,
                    });
                }
                for child in &symbol.children {
                    if name_matches(&child.name, &query) {
                        matches.push(WorkspaceSymbol {
                            symbol: child.clone(),
                            container: Some(symbol.name.to_string()),
                        });
                    }
                }
            }
        }
        matches.sort_by(|left, right| {
            left.symbol
                .path
                .cmp(&right.symbol.path)
                .then(left.symbol.range.start.cmp(&right.symbol.range.start))
        });
        matches
    }

    pub(crate) fn hover(&self, path: &Path, offset: usize) -> Option<HoverInfo> {
        let file = self.files.get(path)?;
        let name = identifier_at(&file.source, offset)?;
        if let Some(symbol) = self.resolve(path, &name, offset) {
            return Some(HoverInfo {
                signature: symbol.signature,
                documentation: symbol.documentation,
            });
        }
        builtin(&name).map(|builtin| HoverInfo {
            signature: builtin
                .overloads
                .iter()
                .map(|overload| overload.signature)
                .collect::<Vec<_>>()
                .join("\n"),
            documentation: builtin
                .overloads
                .iter()
                .find_map(|overload| (!overload.doc.is_empty()).then_some(overload.doc.to_owned())),
        })
    }

    pub(crate) fn completions(&self, path: &Path, offset: usize) -> Vec<Completion> {
        let Some(file) = self.files.get(path) else {
            return Vec::new();
        };
        if file.source[..offset.min(file.source.len())]
            .trim_end()
            .ends_with('.')
        {
            return self.member_completions(path, file, offset);
        }

        let mut completions = Vec::new();
        let mut seen = HashSet::new();
        for local in file
            .locals
            .iter()
            .filter(|local| local.range.start <= offset && local.scope_range.contains(&offset))
        {
            push_completion(
                &mut completions,
                &mut seen,
                Completion {
                    label: local.name.to_string(),
                    kind: CompletionKind::Variable,
                    detail: Some("local variable".to_owned()),
                    insert_text: None,
                    additional_edit: None,
                },
            );
        }
        for symbol in &file.symbols {
            push_completion(&mut completions, &mut seen, symbol_completion(symbol, None));
        }
        for binding in &file.imports {
            if let Some(target_path) = self.module_file(&binding.module)
                && let Some(symbol) = self.files.get(&target_path).and_then(|target| {
                    target
                        .symbols
                        .iter()
                        .find(|symbol| symbol.name == binding.original_name)
                })
            {
                let mut completion = symbol_completion(symbol, None);
                completion.label = binding.local_name.to_string();
                push_completion(&mut completions, &mut seen, completion);
            }
        }
        for (import, _) in &file.oil_imports {
            if let Some((target_path, _)) = self.oil_definition(import)
                && let Some(target) = self.files.get(target_path)
            {
                for symbol in &target.symbols {
                    push_completion(&mut completions, &mut seen, symbol_completion(symbol, None));
                }
            }
        }

        let insertion = import_insertion_offset(&file.source);
        for (candidate_path, candidate) in &self.files {
            if candidate_path == path {
                continue;
            }
            let Some(module) = module_name(&self.root, candidate_path) else {
                continue;
            };
            for symbol in &candidate.symbols {
                if seen.contains(symbol.name.as_str()) {
                    continue;
                }
                let edit = SourceEdit {
                    path: path.to_path_buf(),
                    range: insertion..insertion,
                    new_text: format!("import package::{module}::{};\n", symbol.name),
                };
                push_completion(
                    &mut completions,
                    &mut seen,
                    symbol_completion(symbol, Some(edit)),
                );
            }
        }

        for builtin in BUILTIN_FUNCTIONS {
            push_completion(
                &mut completions,
                &mut seen,
                Completion {
                    label: builtin.name.to_owned(),
                    kind: CompletionKind::Function,
                    detail: builtin
                        .overloads
                        .first()
                        .map(|overload| overload.signature.to_owned()),
                    insert_text: None,
                    additional_edit: None,
                },
            );
        }
        for builtin_type in BUILTIN_TYPES {
            push_completion(
                &mut completions,
                &mut seen,
                Completion {
                    label: (*builtin_type).to_owned(),
                    kind: CompletionKind::Type,
                    detail: Some("WGSL built-in type".to_owned()),
                    insert_text: None,
                    additional_edit: None,
                },
            );
        }
        for (label, insert_text) in KEYWORD_COMPLETIONS {
            push_completion(
                &mut completions,
                &mut seen,
                Completion {
                    label: (*label).to_owned(),
                    kind: CompletionKind::Snippet,
                    detail: Some("WGSL snippet".to_owned()),
                    insert_text: Some((*insert_text).to_owned()),
                    additional_edit: None,
                },
            );
        }
        completions.sort_by(|left, right| left.label.cmp(&right.label));
        completions
    }

    fn member_completions(&self, path: &Path, file: &FileIndex, offset: usize) -> Vec<Completion> {
        let prefix = file.source[..offset.min(file.source.len())].trim_end();
        let Some(dot) = prefix.len().checked_sub(1) else {
            return Vec::new();
        };
        let Some(base_type) = self.infer_member_base_type(path, file, dot) else {
            return Vec::new();
        };
        match base_type {
            Ty::Vector(size, _) => swizzle_labels(size)
                .into_iter()
                .map(|label| Completion {
                    label,
                    kind: CompletionKind::Field,
                    detail: Some("vector swizzle".to_owned()),
                    insert_text: None,
                    additional_edit: None,
                })
                .collect(),
            Ty::Struct(name, fields) => fields
                .into_iter()
                .map(|(label, field_ty)| Completion {
                    label,
                    kind: CompletionKind::Field,
                    detail: Some(format!("{name} field: {field_ty}")),
                    insert_text: None,
                    additional_edit: None,
                })
                .collect(),
            _ => Vec::new(),
        }
    }

    fn infer_member_base_type(&self, path: &Path, file: &FileIndex, dot: usize) -> Option<Ty> {
        let base_source = member_base_source(&file.source, dot)?;
        let expression = base_source.parse::<Expression>().ok()?;
        let module = file.module.as_deref()?;
        let mut visible = file
            .locals
            .iter()
            .filter(|local| local.range.start <= dot && local.scope_range.contains(&dot))
            .collect::<Vec<_>>();
        visible.sort_by_key(|local| local.range.start);
        let mut local_types = HashMap::new();
        for local in visible {
            if let Some(type_name) = declared_type_name(&file.source, local.name.as_str(), dot) {
                local_types.insert(local.name.to_string(), type_name);
            }
        }
        let mut active = HashSet::from([path.to_path_buf()]);
        let imports = self.imported_type_environment(file, &mut active);
        Some(infer_expression_type(
            module,
            expression,
            &local_types,
            imports,
        ))
    }

    fn resolve_member_usage(
        &self,
        path: &Path,
        file: &FileIndex,
        name: &str,
        offset: usize,
    ) -> Option<Symbol> {
        let dot = dot_before_identifier(&file.source, offset)?;
        let Ty::Struct(struct_name, _) = self.infer_member_base_type(path, file, dot)? else {
            return None;
        };
        self.resolve_struct_field(file, &struct_name, name)
    }

    fn resolve_struct_field(
        &self,
        file: &FileIndex,
        struct_name: &str,
        field_name: &str,
    ) -> Option<Symbol> {
        let find_field = |file: &FileIndex, name: &str| {
            file.symbols
                .iter()
                .find(|symbol| symbol.kind == SymbolKind::Struct && symbol.name == name)
                .and_then(|symbol| {
                    symbol
                        .children
                        .iter()
                        .find(|field| field.name == field_name)
                })
                .cloned()
        };
        if let Some(field) = find_field(file, struct_name) {
            return Some(field);
        }
        for binding in &file.imports {
            if binding.local_name != struct_name && binding.original_name != struct_name {
                continue;
            }
            if let Some(target_path) = self.module_file(&binding.module)
                && let Some(field) = self
                    .files
                    .get(&target_path)
                    .and_then(|target| find_field(target, binding.original_name.as_str()))
            {
                return Some(field);
            }
        }
        for (import, _) in &file.oil_imports {
            if let Some((target_path, _)) = self.oil_definition(import)
                && let Some(field) = self
                    .files
                    .get(target_path)
                    .and_then(|target| find_field(target, struct_name))
            {
                return Some(field);
            }
        }
        None
    }

    fn field_declaration_at(&self, file: &FileIndex, name: &str, offset: usize) -> Option<Symbol> {
        file.symbols
            .iter()
            .flat_map(|symbol| &symbol.children)
            .find(|field| {
                field.name == name && field.range.start <= offset && offset <= field.range.end
            })
            .cloned()
    }

    pub(crate) fn type_diagnostics(
        &self,
        path: &Path,
        module: &TranslationUnit,
    ) -> Vec<TypeDiagnostic> {
        let Some(file) = self.files.get(path) else {
            return crate::check_module(module);
        };
        let mut active = HashSet::from([path.to_path_buf()]);
        let imports = self.imported_type_environment(file, &mut active);
        analyze_module(module, imports).0
    }

    fn type_environment(&self, path: &Path, active: &mut HashSet<PathBuf>) -> TypeEnvironment {
        if !active.insert(path.to_path_buf()) {
            return TypeEnvironment::default();
        }
        let environment = self
            .files
            .get(path)
            .and_then(|file| {
                let module = file.module.as_deref()?;
                let imports = self.imported_type_environment(file, active);
                Some(analyze_module(module, imports).1)
            })
            .unwrap_or_default();
        active.remove(path);
        environment
    }

    fn imported_type_environment(
        &self,
        file: &FileIndex,
        active: &mut HashSet<PathBuf>,
    ) -> TypeEnvironment {
        let mut imports = TypeEnvironment::default();
        for binding in &file.imports {
            let Some(target_path) = self.module_file(&binding.module) else {
                continue;
            };
            let target = self.type_environment(&target_path, active);
            imports.bind_alias(
                binding.local_name.as_str(),
                binding.original_name.as_str(),
                &target,
            );
        }
        imports
    }
    pub(crate) fn import_closure(&self, start: &Path) -> Vec<PathBuf> {
        fn visit(
            package: &PackageIndex,
            path: &Path,
            seen: &mut HashSet<PathBuf>,
            output: &mut Vec<PathBuf>,
        ) {
            if !seen.insert(path.to_path_buf()) {
                return;
            }
            output.push(path.to_path_buf());
            let Some(file) = package.files.get(path) else {
                return;
            };
            for module in &file.imported_modules {
                if let Some(next) = package.module_file(module) {
                    visit(package, &next, seen, output);
                }
            }
            for (import, _) in &file.oil_imports {
                if let Some((next, _)) = package.oil_definition(import) {
                    visit(package, next, seen, output);
                }
            }
        }

        let mut output = Vec::new();
        visit(self, start, &mut HashSet::new(), &mut output);
        output
    }

    pub(crate) fn import_cycle(&self, start: &Path) -> Option<Vec<PathBuf>> {
        fn visit(
            package: &PackageIndex,
            path: &Path,
            stack: &mut Vec<PathBuf>,
            active: &mut HashSet<PathBuf>,
            complete: &mut HashSet<PathBuf>,
        ) -> Option<Vec<PathBuf>> {
            if active.contains(path) {
                let start = stack.iter().position(|item| item == path).unwrap_or(0);
                let mut cycle = stack[start..].to_vec();
                cycle.push(path.to_path_buf());
                return Some(cycle);
            }
            if complete.contains(path) {
                return None;
            }
            active.insert(path.to_path_buf());
            stack.push(path.to_path_buf());
            if let Some(file) = package.files.get(path) {
                for module in &file.imported_modules {
                    if let Some(next) = package.module_file(module)
                        && let Some(cycle) = visit(package, &next, stack, active, complete)
                    {
                        return Some(cycle);
                    }
                }
            }
            stack.pop();
            active.remove(path);
            complete.insert(path.to_path_buf());
            None
        }

        visit(
            self,
            start,
            &mut Vec::new(),
            &mut HashSet::new(),
            &mut HashSet::new(),
        )
    }

    fn resolve(&self, path: &Path, name: &str, offset: usize) -> Option<Symbol> {
        let file = self.files.get(path)?;
        if let Some(field) = self.field_declaration_at(file, name, offset) {
            return Some(field);
        }
        if let Some(field) = self.resolve_member_usage(path, file, name, offset) {
            return Some(field);
        }
        if let Some(local) = file
            .locals
            .iter()
            .filter(|local| {
                local.name == name
                    && local.range.start <= offset
                    && local.scope_range.contains(&offset)
            })
            .max_by_key(|local| local.range.start)
        {
            return Some(Symbol {
                name: local.name.clone(),
                kind: SymbolKind::Variable,
                path: path.to_path_buf(),
                range: local.range.clone(),
                full_range: local.range.clone(),
                signature: name.to_owned(),
                documentation: None,
                children: Vec::new(),
            });
        }
        if let Some(symbol) = file.symbols.iter().find(|symbol| symbol.name == name) {
            return Some(symbol.clone());
        }
        if let Some(binding) = file
            .imports
            .iter()
            .find(|binding| binding.local_name == name)
            && let Some(target_path) = self.module_file(&binding.module)
            && let Some(symbol) = self.files.get(&target_path).and_then(|target| {
                target
                    .symbols
                    .iter()
                    .find(|symbol| symbol.name == binding.original_name)
            })
        {
            return Some(symbol.clone());
        }
        for (import, _) in &file.oil_imports {
            if let Some((target_path, _)) = self.oil_definition(import)
                && let Some(symbol) = self
                    .files
                    .get(target_path)
                    .and_then(|target| target.symbols.iter().find(|symbol| symbol.name == name))
            {
                return Some(symbol.clone());
            }
        }
        None
    }

    fn oil_definition(&self, import: &str) -> Option<(&PathBuf, &Range<usize>)> {
        self.files.iter().find_map(|(path, file)| {
            file.oil_definitions
                .iter()
                .find_map(|(definition, range)| (definition == import).then_some((path, range)))
        })
    }

    fn module_file(&self, module: &ModulePath) -> Option<PathBuf> {
        if matches!(module.origin, PathOrigin::Package(_)) {
            return None;
        }
        let mut path = self.root.clone();
        path.extend(&module.components);
        path.set_extension("wesl");
        if self.files.contains_key(&path) {
            return Some(path);
        }
        path.set_extension("wgsl");
        self.files.contains_key(&path).then_some(path)
    }

    /// A symbol's declaration range, valid against `symbol.path`'s CURRENT text.
    /// `resolve` may hand back a `Symbol`/`LocalSymbol` range measured before that
    /// file's last failed reparse (see `FileIndex::positions_stale`); returning it
    /// verbatim would point into whatever new text now occupies those old byte offsets.
    /// When the file is stale, re-derive the range from the identifier's occurrences in
    /// the current text instead. That only succeeds unambiguously: with zero or
    /// multiple occurrences of the name there is no way to tell which one used to be
    /// the declaration, so the caller gets `None` and drops the position rather than
    /// guessing. A single surviving occurrence is still not enough on its own: if the
    /// edit that broke the parse deleted the declaration and left one call site behind,
    /// that lone occurrence is a use, not a declaration, yet it would otherwise pass the
    /// count check. Requiring it to be introduced by a declaring keyword (`fn`,
    /// `struct`, `var`, `let`, `const`, `override`, `alias`) tells the two apart;
    /// anything else -- a call, a type reference, a field access -- returns `None`
    /// instead of mislabelling a use as the definition.
    fn current_range(&self, symbol: &Symbol) -> Option<Range<usize>> {
        let file = self.files.get(&symbol.path)?;
        if !file.positions_stale {
            return Some(symbol.range.clone());
        }
        const DECLARING_KEYWORDS: &[&str] =
            &["fn", "struct", "var", "let", "const", "override", "alias"];
        let name = symbol.name.as_str();
        let all_tokens = tokens(&file.source);
        let mut found: Option<(usize, Range<usize>)> = None;
        for (index, (token, range)) in all_tokens.iter().enumerate() {
            if *token != name {
                continue;
            }
            if found.is_some() {
                return None;
            }
            found = Some((index, range.clone()));
        }
        let (index, range) = found?;
        let declares = index
            .checked_sub(1)
            .and_then(|previous| all_tokens.get(previous))
            .is_some_and(|(token, _)| DECLARING_KEYWORDS.contains(token));
        declares.then_some(range)
    }
}

const KEYWORD_COMPLETIONS: &[(&str, &str)] = &[
    ("fn", "fn ${1:name}(${2}) {\n    ${0}\n}"),
    ("struct", "struct ${1:Name} {\n    ${0}\n}"),
    (
        "@fragment fn",
        "@fragment\nfn ${1:fragment_main}() -> @location(0) vec4<f32> {\n    ${0}\n}",
    ),
    (
        "@vertex fn",
        "@vertex\nfn ${1:vertex_main}() -> @builtin(position) vec4<f32> {\n    ${0}\n}",
    ),
];

fn push_completion(
    output: &mut Vec<Completion>,
    seen: &mut HashSet<String>,
    completion: Completion,
) {
    if seen.insert(completion.label.clone()) {
        output.push(completion);
    }
}

fn symbol_completion(symbol: &Symbol, additional_edit: Option<SourceEdit>) -> Completion {
    Completion {
        label: symbol.name.to_string(),
        kind: match symbol.kind {
            SymbolKind::Function => CompletionKind::Function,
            SymbolKind::Struct => CompletionKind::Struct,
            SymbolKind::Field => CompletionKind::Field,
            SymbolKind::Variable | SymbolKind::Constant | SymbolKind::Override => {
                CompletionKind::Variable
            }
            SymbolKind::Alias => CompletionKind::Type,
        },
        detail: Some(symbol.signature.clone()),
        insert_text: None,
        additional_edit,
    }
}

fn module_name(root: &Path, path: &Path) -> Option<String> {
    let relative = path.strip_prefix(root).ok()?.with_extension("");
    Some(
        relative
            .components()
            .map(|component| component.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("::"),
    )
}

/// Occurrences of a module path inside `import` statements. The match must end on a path
/// boundary, so `package::mesh` never matches inside `package::mesh_utils`.
fn import_path_ranges(source: &str, module_path: &str) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let mut line_start = 0;
    for line in source.split_inclusive('\n') {
        let trimmed = line.trim_start();
        let is_import = trimmed
            .strip_prefix("import")
            .is_some_and(|rest| rest.starts_with(char::is_whitespace));
        if is_import {
            let mut search = 0;
            while let Some(found) = line[search..].find(module_path) {
                let start = search + found;
                let end = start + module_path.len();
                let before_is_path = start
                    .checked_sub(1)
                    .and_then(|index| line.as_bytes().get(index))
                    .is_some_and(|byte| {
                        byte.is_ascii_alphanumeric() || *byte == b'_' || *byte == b':'
                    });
                let after_is_identifier = line
                    .as_bytes()
                    .get(end)
                    .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_');
                if !before_is_path && !after_is_identifier {
                    ranges.push(line_start + start..line_start + end);
                }
                search = end;
            }
        }
        line_start += line.len();
    }
    ranges
}

fn import_insertion_offset(source: &str) -> usize {
    let mut offset = 0;
    let mut last_import_end = 0;
    for line in source.split_inclusive('\n') {
        let trimmed = line.trim_start();
        if trimmed.starts_with("import ") {
            last_import_end = offset + line.len();
        } else if !trimmed.is_empty() && !trimmed.starts_with("//") {
            break;
        }
        offset += line.len();
    }
    last_import_end
}

impl FileIndex {
    fn new(path: PathBuf, source: Arc<str>) -> Self {
        let oil_imports = dialect::imports(&source);
        let oil_definitions = dialect::definitions(&source);
        let dialect = dialect::is_naga_oil(&source);
        let processed = dialect::preprocess(&source);
        let Ok(module) = parse_str(&processed) else {
            return Self {
                module: None,
                source,
                symbols: Vec::new(),
                locals: Vec::new(),
                imports: Vec::new(),
                imported_modules: Vec::new(),
                oil_imports,
                oil_definitions,
                dialect,
                positions_stale: false,
            };
        };
        let symbols = index_symbols(&path, &source, &module);
        let locals = index_locals(&source, &module);
        let (imports, imported_modules) = index_imports(&module);
        Self {
            module: Some(Arc::new(module)),
            source,
            symbols,
            locals,
            imports,
            imported_modules,
            oil_imports,
            oil_definitions,
            dialect,
            positions_stale: false,
        }
    }
}

fn index_symbols(path: &Path, source: &str, module: &TranslationUnit) -> Vec<Symbol> {
    module
        .global_declarations
        .iter()
        .filter_map(|declaration| {
            let ident = declaration.ident()?;
            let name = ident.name().to_string();
            let full_range = declaration.span().range();
            let range = find_identifier(source, full_range.clone(), &name)?;
            let (kind, children) = match declaration.node() {
                GlobalDeclaration::Function(_) => (SymbolKind::Function, Vec::new()),
                GlobalDeclaration::Struct(structure) => {
                    let children = structure
                        .members
                        .iter()
                        .filter_map(|member| {
                            let name = member.ident.name().to_string();
                            let full_range = member.span().range();
                            Some(Symbol {
                                range: find_identifier(source, full_range.clone(), &name)?,
                                name: name.into(),
                                kind: SymbolKind::Field,
                                path: path.to_path_buf(),
                                signature: source[full_range.clone()]
                                    .trim()
                                    .trim_end_matches(',')
                                    .to_owned(),
                                documentation: doc_before(source, full_range.start),
                                full_range,
                                children: Vec::new(),
                            })
                        })
                        .collect();
                    (SymbolKind::Struct, children)
                }
                GlobalDeclaration::TypeAlias(_) => (SymbolKind::Alias, Vec::new()),
                GlobalDeclaration::Declaration(declaration) => match declaration.kind {
                    wgsl_parse::syntax::DeclarationKind::Const => {
                        (SymbolKind::Constant, Vec::new())
                    }
                    wgsl_parse::syntax::DeclarationKind::Override => {
                        (SymbolKind::Override, Vec::new())
                    }
                    _ => (SymbolKind::Variable, Vec::new()),
                },
                GlobalDeclaration::Void | GlobalDeclaration::ConstAssert(_) => return None,
            };
            let signature_end = source[full_range.clone()]
                .find('{')
                .map(|relative| full_range.start + relative)
                .unwrap_or(full_range.end);
            Some(Symbol {
                name: name.into(),
                kind,
                path: path.to_path_buf(),
                range,
                signature: source[full_range.start..signature_end]
                    .trim()
                    .trim_end_matches(';')
                    .trim()
                    .to_owned(),
                documentation: doc_before(source, full_range.start),
                full_range,
                children,
            })
        })
        .collect()
}

fn index_imports(module: &TranslationUnit) -> (Vec<ImportBinding>, Vec<ModulePath>) {
    let mut bindings = Vec::new();
    let mut modules = Vec::new();
    for import in &module.imports {
        let Some(base) = &import.path else {
            continue;
        };
        modules.push(base.clone());
        collect_imports(base, &[], &import.content, &mut bindings);
    }
    (bindings, modules)
}

fn collect_imports(
    base: &ModulePath,
    prefix: &[String],
    content: &ImportContent,
    output: &mut Vec<ImportBinding>,
) {
    match content {
        ImportContent::Item(item) => output.push(binding(base, prefix, item)),
        ImportContent::Collection(imports) => {
            for import in imports {
                let mut next = prefix.to_vec();
                next.extend(import.path.iter().cloned());
                collect_imports(base, &next, &import.content, output);
            }
        }
    }
}

fn binding(base: &ModulePath, prefix: &[String], item: &ImportItem) -> ImportBinding {
    let mut module = base.clone();
    module.components.extend(prefix.iter().cloned());
    ImportBinding {
        local_name: item
            .rename
            .as_ref()
            .unwrap_or(&item.ident)
            .name()
            .as_str()
            .into(),
        original_name: item.ident.name().as_str().into(),
        module,
    }
}

pub(crate) fn brace_scopes(source: &str) -> Vec<Range<usize>> {
    let mut stack = Vec::new();
    let mut scopes = Vec::new();
    for (token, range) in tokens(source) {
        match token {
            "{" => stack.push(range.start),
            "}" => {
                if let Some(start) = stack.pop() {
                    scopes.push(start..range.end);
                }
            }
            _ => {}
        }
    }
    scopes
}

fn innermost_scope(
    scopes: &[Range<usize>],
    offset: usize,
    fallback: &Range<usize>,
) -> Range<usize> {
    scopes
        .iter()
        .filter(|scope| scope.start < offset && offset < scope.end)
        .min_by_key(|scope| scope.end - scope.start)
        .cloned()
        .unwrap_or_else(|| fallback.clone())
}

fn index_locals(source: &str, module: &TranslationUnit) -> Vec<LocalSymbol> {
    let scopes = brace_scopes(source);
    let mut locals = Vec::new();
    for declaration in &module.global_declarations {
        let GlobalDeclaration::Function(function) = declaration.node() else {
            continue;
        };
        let function_range = declaration.span().range();
        let signature_end = source[function_range.clone()]
            .find('{')
            .map(|relative| function_range.start + relative)
            .unwrap_or(function_range.end);
        let mut parameter_start = function_range.start;
        for parameter in &function.parameters {
            let name = parameter.ident.name().to_string();
            let Some(range) = find_identifier(source, parameter_start..signature_end, &name) else {
                continue;
            };
            parameter_start = range.end;
            locals.push(LocalSymbol {
                name: name.into(),
                range,
                scope_range: function_range.clone(),
            });
        }
        collect_compound_locals(
            source,
            &function.body,
            &function_range,
            &scopes,
            &mut locals,
        );
    }
    locals
}

fn collect_compound_locals(
    source: &str,
    compound: &CompoundStatement,
    scope: &Range<usize>,
    scopes: &[Range<usize>],
    locals: &mut Vec<LocalSymbol>,
) {
    for statement in &compound.statements {
        collect_statement_locals(source, statement, scope, scopes, locals);
    }
}

fn compound_scope(
    compound: &CompoundStatement,
    fallback: &Range<usize>,
    scopes: &[Range<usize>],
) -> Range<usize> {
    compound
        .statements
        .first()
        .map(|statement| innermost_scope(scopes, statement.span().start, fallback))
        .unwrap_or_else(|| fallback.clone())
}

fn collect_statement_locals(
    source: &str,
    statement: &StatementNode,
    scope: &Range<usize>,
    scopes: &[Range<usize>],
    locals: &mut Vec<LocalSymbol>,
) {
    match statement.node() {
        Statement::Declaration(declaration) => {
            let name = declaration.ident.name().to_string();
            if let Some(range) = find_identifier(source, statement.span().range(), &name) {
                locals.push(LocalSymbol {
                    name: name.into(),
                    range,
                    scope_range: scope.clone(),
                });
            }
        }
        Statement::Compound(compound) => {
            let child_scope = statement.span().range();
            collect_compound_locals(source, compound, &child_scope, scopes, locals);
        }
        Statement::If(branch) => {
            let branch_scope = compound_scope(&branch.if_clause.body, scope, scopes);
            collect_compound_locals(
                source,
                &branch.if_clause.body,
                &branch_scope,
                scopes,
                locals,
            );
            for clause in &branch.else_if_clauses {
                let clause_scope = compound_scope(&clause.body, scope, scopes);
                collect_compound_locals(source, &clause.body, &clause_scope, scopes, locals);
            }
            if let Some(clause) = &branch.else_clause {
                let clause_scope = compound_scope(&clause.body, scope, scopes);
                collect_compound_locals(source, &clause.body, &clause_scope, scopes, locals);
            }
        }
        Statement::Switch(switch) => {
            for clause in &switch.clauses {
                let clause_scope = compound_scope(&clause.body, scope, scopes);
                collect_compound_locals(source, &clause.body, &clause_scope, scopes, locals);
            }
        }
        Statement::Loop(loop_statement) => {
            let body_scope = compound_scope(&loop_statement.body, scope, scopes);
            collect_compound_locals(source, &loop_statement.body, &body_scope, scopes, locals);
            if let Some(continuing) = &loop_statement.continuing {
                let continuing_scope = compound_scope(&continuing.body, scope, scopes);
                collect_compound_locals(
                    source,
                    &continuing.body,
                    &continuing_scope,
                    scopes,
                    locals,
                );
            }
        }
        Statement::For(for_statement) => {
            let loop_scope = statement.span().range();
            if let Some(initializer) = &for_statement.initializer {
                collect_statement_locals(source, initializer, &loop_scope, scopes, locals);
            }
            let body_scope = compound_scope(&for_statement.body, &loop_scope, scopes);
            collect_compound_locals(source, &for_statement.body, &body_scope, scopes, locals);
        }
        Statement::While(while_statement) => {
            let body_scope = compound_scope(&while_statement.body, scope, scopes);
            collect_compound_locals(source, &while_statement.body, &body_scope, scopes, locals);
        }
        _ => {}
    }
}

fn dot_before_identifier(source: &str, offset: usize) -> Option<usize> {
    let prefix = source.get(..offset)?;
    let trimmed = prefix.trim_end();
    trimmed.ends_with('.').then(|| trimmed.len() - 1)
}

fn member_base_source(source: &str, dot: usize) -> Option<&str> {
    if source.as_bytes().get(dot) != Some(&b'.') {
        return None;
    }
    let bytes = source.as_bytes();
    let mut cursor = dot;
    let mut parentheses = 0_u32;
    let mut brackets = 0_u32;
    while cursor > 0 {
        let byte = bytes[cursor - 1];
        match byte {
            b')' => parentheses += 1,
            b']' => brackets += 1,
            b'(' if parentheses > 0 => parentheses -= 1,
            b'[' if brackets > 0 => brackets -= 1,
            b';' | b'{' | b'}' | b'=' | b',' | b'\n' if parentheses == 0 && brackets == 0 => {
                break;
            }
            b'(' if parentheses == 0 && brackets == 0 => break,
            _ => {}
        }
        cursor -= 1;
    }
    let expression = source[cursor..dot].trim();
    (!expression.is_empty()).then_some(expression)
}

fn swizzle_labels(size: u8) -> Vec<String> {
    fn extend(labels: &mut Vec<String>, alphabet: &[u8], prefix: &mut String, remaining: u8) {
        if remaining == 0 {
            labels.push(prefix.clone());
            return;
        }
        for component in alphabet {
            prefix.push(char::from(*component));
            extend(labels, alphabet, prefix, remaining - 1);
            prefix.pop();
        }
    }

    let mut labels = Vec::new();
    for alphabet in [
        &b"xyzw"[..size.min(4) as usize],
        &b"rgba"[..size.min(4) as usize],
    ] {
        for length in 1..=4 {
            extend(&mut labels, alphabet, &mut String::new(), length);
        }
    }
    labels
}

fn declared_type_name(source: &str, name: &str, before: usize) -> Option<String> {
    let tokens = tokens(source);
    let mut result = None;
    for (index, (token, range)) in tokens.iter().enumerate() {
        if *token != name || range.start >= before {
            continue;
        }
        if tokens
            .get(index + 1)
            .is_some_and(|(token, _)| *token == ":")
        {
            let start = tokens.get(index + 2)?.1.start;
            let mut end = start;
            let mut template_depth = 0_u32;
            for (token, range) in tokens.iter().skip(index + 2) {
                match *token {
                    "<" => template_depth += 1,
                    ">" if template_depth > 0 => template_depth -= 1,
                    "=" | "," | ";" | ")" | "{" if template_depth == 0 => {
                        break;
                    }
                    _ => {}
                }
                end = range.end;
            }
            result = (end > start).then(|| source[start..end].trim().to_owned());
        } else if tokens
            .get(index + 1)
            .is_some_and(|(token, _)| *token == "=")
        {
            result = tokens.get(index + 2).and_then(|(token, _)| {
                (token.starts_with("vec")
                    || token.starts_with("mat")
                    || token.chars().next().is_some_and(char::is_uppercase))
                .then(|| (*token).to_owned())
            });
        }
    }
    result
}

fn identifier_at(source: &str, offset: usize) -> Option<String> {
    identifier_range_at(source, offset).map(|(name, _)| name)
}

fn identifier_range_at(source: &str, offset: usize) -> Option<(String, Range<usize>)> {
    let identifiers = tokens(source)
        .into_iter()
        .filter(|(token, _)| is_identifier(token))
        .collect::<Vec<_>>();
    identifiers
        .iter()
        .find(|(_, range)| range.start <= offset && offset < range.end)
        .or_else(|| identifiers.iter().find(|(_, range)| range.end == offset))
        .map(|(token, range)| ((*token).to_owned(), range.clone()))
}

fn identifier_ranges(source: &str, expected: &str) -> Vec<Range<usize>> {
    tokens(source)
        .into_iter()
        .filter_map(|(token, range)| (expected.is_empty() || token == expected).then_some(range))
        .collect()
}

pub(crate) fn tokens(source: &str) -> Vec<(&str, Range<usize>)> {
    let bytes = source.as_bytes();
    let mut output = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index..].starts_with(b"//") {
            index += 2;
            while index < bytes.len() && bytes[index] != b'\n' {
                index += 1;
            }
            continue;
        }
        if bytes[index..].starts_with(b"/*") {
            index += 2;
            while index + 1 < bytes.len() && !bytes[index..].starts_with(b"*/") {
                index += 1;
            }
            index = (index + 2).min(bytes.len());
            continue;
        }
        if bytes[index].is_ascii_alphabetic() || bytes[index] == b'_' {
            let start = index;
            index += 1;
            while index < bytes.len()
                && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_')
            {
                index += 1;
            }
            output.push((&source[start..index], start..index));
        } else {
            let start = index;
            index += 1;
            if !bytes[start].is_ascii_whitespace() {
                output.push((&source[start..index], start..index));
            }
        }
    }
    output
}

fn find_identifier(source: &str, range: Range<usize>, name: &str) -> Option<Range<usize>> {
    identifier_ranges(&source[range.clone()], name)
        .into_iter()
        .next()
        .map(|found| range.start + found.start..range.start + found.end)
}

fn doc_before(source: &str, offset: usize) -> Option<String> {
    let before = &source[..offset];
    let mut lines = before.lines().rev();
    let mut docs = Vec::new();
    for line in &mut lines {
        let trimmed = line.trim();
        if let Some(comment) = trimmed.strip_prefix("//") {
            docs.push(comment.trim().to_owned());
        } else if trimmed.is_empty() && docs.is_empty() {
            continue;
        } else {
            break;
        }
    }
    (!docs.is_empty()).then(|| {
        docs.reverse();
        docs.join("\n")
    })
}

fn is_shader(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("wesl" | "wgsl")
    )
}

pub(crate) fn is_identifier(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes
        .next()
        .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

/// One hint per struct member giving its byte offset, alignment and size. Anchored at the end
/// of the member declaration, so it reads as a trailing annotation on the field.
fn struct_layout_hints(
    module: &TranslationUnit,
    types: TypeEnvironment,
    range: &Range<usize>,
) -> Vec<InlayHint> {
    let overrides = struct_member_overrides(module);
    let resolved = collect_struct_types(module, types)
        .into_iter()
        .collect::<HashMap<_, _>>();
    let mut hints = Vec::new();
    for declaration in &module.global_declarations {
        let GlobalDeclaration::Struct(structure) = declaration.node() else {
            continue;
        };
        let name = structure.ident.name().to_string();
        let Some(fields) = resolved.get(&name) else {
            continue;
        };
        let Some(members) = struct_layout(&name, fields, &overrides) else {
            continue;
        };
        for (member, layout) in structure.members.iter().zip(members) {
            let anchor = member.span().range().end;
            if !range.contains(&anchor) {
                continue;
            }
            hints.push(InlayHint {
                offset: anchor,
                label: format!(
                    "offset {}, align {}, size {}",
                    layout.offset, layout.align, layout.size
                ),
                kind: InlayKind::Layout,
            });
        }
    }
    hints
}

/// `@align`/`@size` attributes per struct member, in declaration order.
pub(crate) fn struct_member_overrides(module: &TranslationUnit) -> MemberOverrides {
    module
        .global_declarations
        .iter()
        .filter_map(|declaration| {
            let GlobalDeclaration::Struct(structure) = declaration.node() else {
                return None;
            };
            let members = structure
                .members
                .iter()
                .map(|member| {
                    let mut align = None;
                    let mut size = None;
                    for attribute in &member.attributes {
                        match attribute.node() {
                            Attribute::Align(expression) => align = const_u32(module, expression),
                            Attribute::Size(expression) => size = const_u32(module, expression),
                            _ => {}
                        }
                    }
                    (align, size)
                })
                .collect();
            Some((structure.ident.name().to_string(), members))
        })
        .collect()
}

/// Structs are callable as constructors, but their indexed signature stops at the opening
/// brace, so rebuild a call-shaped label from the members.
fn struct_constructor_label(symbol: &Symbol) -> String {
    format!(
        "{}({})",
        symbol.name,
        symbol
            .children
            .iter()
            .map(|member| member.signature.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    )
}

/// Case-insensitive substring match. An empty query matches everything, which is what
/// clients that populate the symbol picker before the user types expect.
fn name_matches(name: &str, lowercase_query: &str) -> bool {
    lowercase_query.is_empty() || name.to_lowercase().contains(lowercase_query)
}

fn is_builtin(name: &str) -> bool {
    builtin(name).is_some() || BUILTIN_TYPES.contains(&name)
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, path::PathBuf, sync::Arc};

    use super::{FileIndex, PackageIndex, declared_type_name, member_base_source};
    use crate::ty::{Ty, TypeEnvironment, infer_expression_type};

    #[test]
    fn recovers_expression_type_from_last_good_index() {
        let good = "struct Item { value: f32, }\nfn main() { let v: vec3<f32> = vec3(0.0); var item: Item; let q = v; }";
        let mut file = FileIndex::new(PathBuf::from("shader.wesl"), Arc::from(good));
        let edited = good.replace("let q = v;", "v.");
        file.source = Arc::from(edited.clone());
        let dot = edited.find("v.").unwrap() + 1;
        let base = member_base_source(&edited, dot).unwrap();
        assert_eq!(base, "v");
        let type_name = declared_type_name(&edited, "v", dot).unwrap();
        assert_eq!(type_name, "vec3<f32>");
        let expression = base.parse().unwrap();
        let ty = infer_expression_type(
            file.module.as_deref().unwrap(),
            expression,
            &HashMap::from([("v".to_owned(), type_name)]),
            TypeEnvironment::default(),
        );
        assert_eq!(ty, Ty::Vector(3, Box::new(Ty::F32)));
    }

    #[test]
    fn rename_after_failed_reparse_never_touches_stale_ranges() {
        let root = PathBuf::from("/virtual/rename-stale");
        let path = root.join("shader.wesl");
        let good = "fn double(x: f32) -> f32 { return x * 2.0; }\nfn use_double() -> f32 { return double(3.0); }\n";
        let overlays = HashMap::from([(path.clone(), Arc::<str>::from(good))]);
        let mut package = PackageIndex::build(root, &overlays);

        // Prepending and appending text shifts every byte offset the first parse
        // recorded, and the appended `fn half(` is unterminated, so this reparse fails
        // -- the review's repro: the index keeps `double`'s OLD declaration range
        // (3..9) while `source` moves on to text where those same bytes now spell
        // "pad\nfn".
        let edited = format!("// pad\n{good}fn half(");
        assert_eq!(&edited[3..9], "pad\nfn");
        package.update(path.clone(), Arc::from(edited.clone()));

        let offset = edited.find("double").unwrap() + 1;
        let edits = package
            .rename(&path, offset, "triple")
            .expect("double is a renameable, non-builtin identifier");

        assert_eq!(
            edits.len(),
            2,
            "the declaration and the one call site: {edits:#?}"
        );
        for edit in &edits {
            assert_eq!(edit.path, path);
            assert_eq!(
                &edited[edit.range.clone()],
                "double",
                "every renamed range must literally spell the identifier in CURRENT text"
            );
            assert_ne!(
                edit.range,
                3..9,
                "must not reuse the stale pre-reparse declaration range"
            );
        }
    }

    #[test]
    fn stale_file_declaration_positions_are_recovered_or_omitted() {
        let root = PathBuf::from("/virtual/definition-stale");
        let path = root.join("shader.wesl");
        let good = "fn solo(x: f32) -> f32 { return x * 2.0; }\n";
        let overlays = HashMap::from([(path.clone(), Arc::<str>::from(good))]);
        let mut package = PackageIndex::build(root, &overlays);

        let edited = format!("// pad\n{good}fn half(");
        package.update(path.clone(), Arc::from(edited.clone()));

        // `solo` appears exactly once in the file, so the declaration range is
        // unambiguously recoverable from current text even though the index is stale.
        let fresh_offset = edited.find("solo").unwrap() + 1;
        let location = package
            .definition(&path, fresh_offset)
            .expect("solo still resolves to its own (stale-index) declaration");
        assert_eq!(location.path, path);
        assert_eq!(&edited[location.range.clone()], "solo");
        assert_ne!(
            location.range,
            3..7,
            "must not reuse the stale pre-reparse declaration range"
        );

        // Bulk listing endpoints have no way to re-derive a whole symbol tree without a
        // parse, so they drop stale files outright rather than surface any of their
        // ranges.
        assert!(package.document_symbols(&path).is_empty());
        assert!(package.workspace_symbols("solo").is_empty());
    }

    #[test]
    fn current_range_does_not_promote_a_surviving_call_site_to_a_declaration() {
        let root = PathBuf::from("/virtual/deleted-declaration");
        let path = root.join("shader.wesl");
        let good = "fn helper() -> f32 { return 1.0; }\nfn main() -> f32 { return helper(); }\n";
        let overlays = HashMap::from([(path.clone(), Arc::<str>::from(good))]);
        let mut package = PackageIndex::build(root, &overlays);

        let declaration_offset = good.find("helper").unwrap() + 1;
        assert!(
            package.definition(&path, declaration_offset).is_some(),
            "the declaration must resolve before the edit"
        );

        // Deletes `helper`'s declaration outright and leaves the brace unterminated, so the
        // reparse fails -- the reviewer's repro. `helper` now occurs exactly once in the
        // text (the call), which used to be enough for `current_range`'s exactly-once rule
        // to (wrongly) hand it back as the declaration.
        let edited = "fn main() -> f32 { return helper(); ";
        package.update(path.clone(), Arc::from(edited));

        let call_offset = edited.find("helper").unwrap() + 1;
        assert!(
            package.definition(&path, call_offset).is_none(),
            "a lone call site must not be promoted to a declaration"
        );

        // The call site is still a legitimate reference -- it just cannot masquerade as
        // the declaration entry.
        let references = package.references(&path, call_offset, true);
        assert_eq!(
            references.len(),
            1,
            "the call site itself, and nothing claiming to be the declaration: {references:#?}"
        );
        assert_eq!(&edited[references[0].range.clone()], "helper");
    }
}
