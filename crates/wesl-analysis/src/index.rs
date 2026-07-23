use std::{
    collections::{HashMap, HashSet},
    fs,
    ops::Range,
    path::{Path, PathBuf},
    sync::Arc,
};

use smol_str::SmolStr;
use walkdir::WalkDir;
use wgsl_parse::{
    SyntaxNode, parse_str,
    syntax::{
        GlobalDeclaration, ImportContent, ImportItem, ModulePath, PathOrigin, TranslationUnit,
    },
};

use crate::{
    builtins::{BUILTIN_FUNCTIONS, BUILTIN_TYPES, builtin},
    dialect,
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
    symbols: Vec<Symbol>,
    locals: Vec<LocalSymbol>,
    imports: Vec<ImportBinding>,
    imported_modules: Vec<ModulePath>,
    oil_imports: Vec<(String, Range<usize>)>,
    oil_definitions: Vec<(String, Range<usize>)>,
}

#[derive(Clone, Debug)]
struct LocalSymbol {
    name: SmolStr,
    range: Range<usize>,
    function_range: Range<usize>,
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
        self.resolve(path, &name, offset).map(|symbol| Location {
            path: symbol.path.clone(),
            range: symbol.range.clone(),
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
        let mut locations = Vec::new();
        if include_declaration {
            locations.push(Location {
                path: target.path.clone(),
                range: target.range.clone(),
            });
        }
        for (candidate_path, candidate) in &self.files {
            for range in identifier_ranges(&candidate.source, &name) {
                if candidate_path == &target.path && range == target.range {
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

    pub(crate) fn document_symbols(&self, path: &Path) -> Vec<Symbol> {
        self.files
            .get(path)
            .map(|file| file.symbols.clone())
            .unwrap_or_default()
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
            return self.member_completions(file, offset);
        }

        let mut completions = Vec::new();
        let mut seen = HashSet::new();
        for local in file
            .locals
            .iter()
            .filter(|local| local.range.start <= offset && local.function_range.contains(&offset))
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

    fn member_completions(&self, file: &FileIndex, offset: usize) -> Vec<Completion> {
        let prefix = file.source[..offset.min(file.source.len())].trim_end();
        let Some(dot) = prefix.len().checked_sub(1) else {
            return Vec::new();
        };
        let Some(base) = identifier_before(&file.source, dot) else {
            return Vec::new();
        };
        let Some(type_name) = declared_type_name(&file.source, &base, dot) else {
            return Vec::new();
        };
        if let Some(size) = type_name
            .strip_prefix("vec")
            .and_then(|size| size.chars().next())
            .and_then(|size| size.to_digit(10))
        {
            let xyzw = ["x", "y", "z", "w"];
            let rgba = ["r", "g", "b", "a"];
            let mut labels = Vec::new();
            for index in 0..size.min(4) as usize {
                labels.push(xyzw[index]);
                labels.push(rgba[index]);
            }
            if size >= 2 {
                labels.extend(["xy", "rg"]);
            }
            if size >= 3 {
                labels.extend(["xyz", "rgb"]);
            }
            if size >= 4 {
                labels.extend(["xyzw", "rgba"]);
            }
            return labels
                .into_iter()
                .map(|label| Completion {
                    label: label.to_owned(),
                    kind: CompletionKind::Field,
                    detail: Some("vector swizzle".to_owned()),
                    insert_text: None,
                    additional_edit: None,
                })
                .collect();
        }
        if let Some(structure) = self
            .files
            .values()
            .flat_map(|candidate| &candidate.symbols)
            .find(|symbol| symbol.kind == SymbolKind::Struct && symbol.name.as_str() == type_name)
        {
            return structure
                .children
                .iter()
                .map(|field| symbol_completion(field, None))
                .collect();
        }
        self.files
            .values()
            .find_map(|candidate| textual_struct_fields(&candidate.source, &type_name))
            .unwrap_or_default()
            .into_iter()
            .map(|label| Completion {
                label,
                kind: CompletionKind::Field,
                detail: Some(format!("{type_name} field")),
                insert_text: None,
                additional_edit: None,
            })
            .collect()
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
        if let Some(local) = file
            .locals
            .iter()
            .filter(|local| {
                local.name == name
                    && local.range.start <= offset
                    && local.function_range.contains(&offset)
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
        let processed = dialect::preprocess(&source);
        let Ok(module) = parse_str(&processed) else {
            return Self {
                source,
                symbols: Vec::new(),
                locals: Vec::new(),
                imports: Vec::new(),
                imported_modules: Vec::new(),
                oil_imports,
                oil_definitions,
            };
        };
        let symbols = index_symbols(&path, &source, &module);
        let locals = index_locals(&source, &symbols);
        let (imports, imported_modules) = index_imports(&module);
        Self {
            source,
            symbols,
            locals,
            imports,
            imported_modules,
            oil_imports,
            oil_definitions,
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

fn index_locals(source: &str, symbols: &[Symbol]) -> Vec<LocalSymbol> {
    let tokens = tokens(source);
    let mut locals = Vec::new();
    for function in symbols
        .iter()
        .filter(|symbol| symbol.kind == SymbolKind::Function)
    {
        let function_tokens: Vec<_> = tokens
            .iter()
            .filter(|(_, range)| {
                range.start >= function.full_range.start && range.end <= function.full_range.end
            })
            .collect();
        for pair in function_tokens.windows(2) {
            let (keyword, _) = pair[0];
            let (name, range) = pair[1];
            if matches!(*keyword, "let" | "var" | "const") {
                locals.push(LocalSymbol {
                    name: (*name).into(),
                    range: range.clone(),
                    function_range: function.full_range.clone(),
                });
            }
        }
        if let Some(open) = source[function.range.end..function.full_range.end].find('(') {
            let start = function.range.end + open + 1;
            if let Some(close) = source[start..function.full_range.end].find(')') {
                let end = start + close;
                for (index, (name, range)) in tokens.iter().enumerate() {
                    if range.start < start || range.end > end {
                        continue;
                    }
                    if tokens
                        .get(index + 1)
                        .is_some_and(|(token, _)| *token == ":")
                    {
                        locals.push(LocalSymbol {
                            name: (*name).into(),
                            range: range.clone(),
                            function_range: function.full_range.clone(),
                        });
                    }
                }
            }
        }
    }
    locals
}

fn textual_struct_fields(source: &str, name: &str) -> Option<Vec<String>> {
    let declaration = format!("struct {name}");
    let start = source.find(&declaration)? + declaration.len();
    let open = start + source[start..].find('{')? + 1;
    let close = open + source[open..].find('}')?;
    let body_tokens = tokens(&source[open..close]);
    Some(
        body_tokens
            .windows(2)
            .filter(|pair| pair[1].0 == ":")
            .map(|pair| pair[0].0.to_owned())
            .collect(),
    )
}

fn identifier_before(source: &str, end: usize) -> Option<String> {
    let bytes = source.as_bytes();
    let mut cursor = end.min(bytes.len());
    while cursor > 0 && bytes[cursor - 1].is_ascii_whitespace() {
        cursor -= 1;
    }
    let identifier_end = cursor;
    while cursor > 0 && (bytes[cursor - 1].is_ascii_alphanumeric() || bytes[cursor - 1] == b'_') {
        cursor -= 1;
    }
    (cursor < identifier_end).then(|| source[cursor..identifier_end].to_owned())
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
            result = tokens.get(index + 2).map(|(token, _)| (*token).to_owned());
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
    identifier_ranges(source, "")
        .into_iter()
        .find(|range| range.start <= offset && offset <= range.end)
        .map(|range| source[range].to_owned())
}

fn identifier_ranges(source: &str, expected: &str) -> Vec<Range<usize>> {
    tokens(source)
        .into_iter()
        .filter_map(|(token, range)| (expected.is_empty() || token == expected).then_some(range))
        .collect()
}

fn tokens(source: &str) -> Vec<(&str, Range<usize>)> {
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

fn is_identifier(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes
        .next()
        .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

fn is_builtin(name: &str) -> bool {
    builtin(name).is_some() || BUILTIN_TYPES.contains(&name)
}
