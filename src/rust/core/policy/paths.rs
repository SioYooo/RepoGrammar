//! Repository-relative path policy shared across boundaries.

use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepoRelativePathError {
    Empty,
    Absolute,
    Backslash,
    ControlCharacter,
    UriLike,
    Traversal,
}

pub fn validate_repo_relative_path(path: &str) -> Result<(), RepoRelativePathError> {
    if path.trim().is_empty() {
        return Err(RepoRelativePathError::Empty);
    }
    if path.chars().any(char::is_control) {
        return Err(RepoRelativePathError::ControlCharacter);
    }
    if path.contains('\\') {
        return Err(RepoRelativePathError::Backslash);
    }
    if path.contains("://") {
        return Err(RepoRelativePathError::UriLike);
    }
    if Path::new(path).is_absolute() || looks_like_windows_absolute_path(path) {
        return Err(RepoRelativePathError::Absolute);
    }
    if path
        .split('/')
        .any(|component| component.is_empty() || component == "." || component == "..")
    {
        return Err(RepoRelativePathError::Traversal);
    }
    for component in Path::new(path).components() {
        match component {
            Component::Normal(_) => {}
            Component::CurDir
            | Component::ParentDir
            | Component::Prefix(_)
            | Component::RootDir => return Err(RepoRelativePathError::Traversal),
        }
    }
    Ok(())
}

pub fn repo_relative_path_buf(path: &str) -> Result<PathBuf, RepoRelativePathError> {
    validate_repo_relative_path(path)?;
    Ok(path.split('/').collect())
}

pub fn looks_like_windows_absolute_path(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_repo_relative_paths() {
        assert!(validate_repo_relative_path("src/a.ts").is_ok());
        assert_eq!(
            repo_relative_path_buf("src/a.ts").expect("path"),
            PathBuf::from("src/a.ts")
        );
    }

    #[test]
    fn rejects_absolute_traversal_uri_backslash_control_and_empty_paths() {
        for (path, error) in [
            ("", RepoRelativePathError::Empty),
            ("   ", RepoRelativePathError::Empty),
            ("/repo/src/a.ts", RepoRelativePathError::Absolute),
            ("C:/repo/src/a.ts", RepoRelativePathError::Absolute),
            ("src\\a.ts", RepoRelativePathError::Backslash),
            ("file://src/a.ts", RepoRelativePathError::UriLike),
            ("src/../a.ts", RepoRelativePathError::Traversal),
            ("src/./a.ts", RepoRelativePathError::Traversal),
            ("src//a.ts", RepoRelativePathError::Traversal),
            ("src/\0/a.ts", RepoRelativePathError::ControlCharacter),
        ] {
            assert_eq!(
                validate_repo_relative_path(path),
                Err(error),
                "expected {path:?} to be rejected as {error:?}"
            );
        }
    }
}
