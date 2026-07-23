use std::{
    borrow::Cow,
    collections::HashMap,
    path::{Path, PathBuf},
};

use wesl::{FileResolver, ResolveError, Resolver};
use wgsl_parse::syntax::{ModulePath, PathOrigin};

pub struct OverlayResolver {
    root: PathBuf,
    buffers: HashMap<PathBuf, String>,
    files: FileResolver,
}

impl OverlayResolver {
    pub fn new(root: impl AsRef<Path>) -> Self {
        let root = root.as_ref().to_path_buf();
        Self {
            files: FileResolver::new(&root),
            root,
            buffers: HashMap::new(),
        }
    }

    pub fn set_buffer(&mut self, path: PathBuf, source: String) {
        self.buffers.insert(path, source);
    }

    pub fn module_path(&self, path: &Path) -> Option<ModulePath> {
        let relative = path.strip_prefix(&self.root).ok()?.with_extension("");
        Some(ModulePath::new(
            PathOrigin::Absolute,
            relative
                .components()
                .map(|component| component.as_os_str().to_string_lossy().into_owned())
                .collect(),
        ))
    }

    fn source_path(&self, path: &ModulePath) -> Option<PathBuf> {
        if path.origin.is_package() {
            return None;
        }
        let mut file = self.root.clone();
        file.extend(&path.components);
        file.set_extension("wesl");
        if self.buffers.contains_key(&file) || file.is_file() {
            return Some(file);
        }
        file.set_extension("wgsl");
        (self.buffers.contains_key(&file) || file.is_file()).then_some(file)
    }
}

impl Resolver for OverlayResolver {
    fn resolve_source<'a>(&'a self, path: &ModulePath) -> Result<Cow<'a, str>, ResolveError> {
        if let Some(file) = self.source_path(path)
            && let Some(source) = self.buffers.get(&file)
        {
            return Ok(Cow::Borrowed(source));
        }
        self.files.resolve_source(path)
    }

    fn display_name(&self, path: &ModulePath) -> Option<String> {
        self.source_path(path)
            .map(|file| file.display().to_string())
            .or_else(|| self.files.display_name(path))
    }

    fn fs_path(&self, path: &ModulePath) -> Option<PathBuf> {
        self.source_path(path).or_else(|| self.files.fs_path(path))
    }
}
