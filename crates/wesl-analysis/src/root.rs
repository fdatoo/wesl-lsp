use std::{
    fs,
    path::{Path, PathBuf},
};

pub fn discover_root(document: &Path, override_root: Option<&Path>) -> PathBuf {
    if let Some(root) = override_root {
        return root.to_path_buf();
    }

    let directory = document.parent().unwrap_or(document);
    for ancestor in directory.ancestors() {
        if ancestor.join("wesl.toml").is_file() {
            return ancestor.to_path_buf();
        }
    }

    let mut root = directory.to_path_buf();
    if !contains_shader(&root) {
        return root;
    }
    for ancestor in directory.ancestors().skip(1) {
        if contains_shader(ancestor) {
            root = ancestor.to_path_buf();
        } else {
            break;
        }
    }
    root
}

fn contains_shader(directory: &Path) -> bool {
    fs::read_dir(directory).is_ok_and(|entries| {
        entries.filter_map(Result::ok).any(|entry| {
            entry.file_type().is_ok_and(|kind| kind.is_file())
                && matches!(
                    entry
                        .path()
                        .extension()
                        .and_then(|extension| extension.to_str()),
                    Some("wesl" | "wgsl")
                )
        })
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::discover_root;

    #[test]
    fn marker_has_priority() {
        let temp = tempdir().unwrap();
        let root = temp.path().join("package");
        let nested = root.join("shaders/nested");
        fs::create_dir_all(&nested).unwrap();
        fs::write(root.join("wesl.toml"), "").unwrap();
        fs::write(nested.join("shader.wesl"), "").unwrap();
        assert_eq!(discover_root(&nested.join("shader.wesl"), None), root);
    }

    #[test]
    fn contiguous_shader_directories_define_root() {
        let temp = tempdir().unwrap();
        let root = temp.path().join("shaders");
        let nested = root.join("effects");
        fs::create_dir_all(&nested).unwrap();
        fs::write(root.join("common.wesl"), "").unwrap();
        fs::write(nested.join("bloom.wesl"), "").unwrap();
        assert_eq!(discover_root(&nested.join("bloom.wesl"), None), root);
    }
}
