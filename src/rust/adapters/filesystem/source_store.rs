//! Filesystem-backed transient source reads for indexing.

use crate::adapters::filesystem::discovery::sha256_hex;
use crate::core::model::ContentHash;
use crate::ports::source_store::{SourceReadRequest, SourceStore, SourceStoreError, SourceText};
use std::fs;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Default)]
pub struct FilesystemSourceStore;

impl SourceStore for FilesystemSourceStore {
    fn read_source(&self, request: SourceReadRequest) -> Result<SourceText, SourceStoreError> {
        read_repository_source(request)
    }
}

fn read_repository_source(request: SourceReadRequest) -> Result<SourceText, SourceStoreError> {
    if request.repository_root.trim().is_empty() {
        return Err(SourceStoreError::InvalidRequest(
            "repository root must not be empty".to_string(),
        ));
    }
    validate_repo_relative_path(&request.path)?;

    let root = PathBuf::from(&request.repository_root);
    let root_metadata = fs::symlink_metadata(&root).map_err(|_| {
        SourceStoreError::InvalidRequest("repository root is not readable".to_string())
    })?;
    if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
        return Err(SourceStoreError::InvalidRequest(
            "repository root must be a real directory".to_string(),
        ));
    }
    let canonical_root = fs::canonicalize(&root).map_err(|_| {
        SourceStoreError::InvalidRequest("repository root is not readable".to_string())
    })?;

    let source_path = root.join(repo_relative_path(&request.path)?);
    let metadata = fs::symlink_metadata(&source_path)
        .map_err(|_| SourceStoreError::Missing(format!("source is missing: {}", request.path)))?;
    if metadata.file_type().is_symlink() {
        return Err(SourceStoreError::InvalidRequest(format!(
            "source must not be a symlink: {}",
            request.path
        )));
    }
    if !metadata.is_file() {
        return Err(SourceStoreError::InvalidRequest(format!(
            "source is not a regular file: {}",
            request.path
        )));
    }
    if metadata.len() > request.max_file_bytes {
        return Err(SourceStoreError::TooLarge(format!(
            "source exceeds configured size limit: {}",
            request.path
        )));
    }

    let canonical_source = fs::canonicalize(&source_path)
        .map_err(|_| SourceStoreError::Unavailable("failed to canonicalize source".to_string()))?;
    if !canonical_source.starts_with(&canonical_root) {
        return Err(SourceStoreError::InvalidRequest(format!(
            "source escapes repository root: {}",
            request.path
        )));
    }

    let bytes = fs::read(&canonical_source).map_err(|_| {
        SourceStoreError::Unavailable(format!("failed to read source: {}", request.path))
    })?;
    if bytes.len() as u64 > request.max_file_bytes {
        return Err(SourceStoreError::TooLarge(format!(
            "source exceeds configured size limit: {}",
            request.path
        )));
    }
    let content_hash = ContentHash::new(format!("sha256:{}", sha256_hex(&bytes)))
        .expect("sha256_hex returns strict sha256:<64 hex chars> payload");
    if content_hash != request.expected_content_hash {
        return Err(SourceStoreError::HashMismatch(format!(
            "source content changed after discovery: {}",
            request.path
        )));
    }
    let text = String::from_utf8(bytes)
        .map_err(|_| SourceStoreError::NonUtf8(format!("source is not UTF-8: {}", request.path)))?;

    Ok(SourceText {
        path: request.path,
        content_hash,
        text,
    })
}

fn validate_repo_relative_path(path: &str) -> Result<(), SourceStoreError> {
    if path.trim().is_empty() {
        return Err(SourceStoreError::InvalidRequest(
            "source path must not be empty".to_string(),
        ));
    }
    if Path::new(path).is_absolute() {
        return Err(SourceStoreError::InvalidRequest(
            "source path must be repository-relative".to_string(),
        ));
    }
    if path.contains('\\') || looks_like_windows_absolute_path(path) {
        return Err(SourceStoreError::InvalidRequest(
            "source path must be repository-relative".to_string(),
        ));
    }
    for component in Path::new(path).components() {
        match component {
            Component::Normal(_) => {}
            Component::CurDir
            | Component::ParentDir
            | Component::Prefix(_)
            | Component::RootDir => {
                return Err(SourceStoreError::InvalidRequest(
                    "source path must not traverse outside repository".to_string(),
                ));
            }
        }
    }
    Ok(())
}

fn looks_like_windows_absolute_path(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

fn repo_relative_path(path: &str) -> Result<PathBuf, SourceStoreError> {
    validate_repo_relative_path(path)?;
    Ok(path.split('/').collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TempWorkspace;

    fn request(workspace: &TempWorkspace, path: &str, bytes: &[u8]) -> SourceReadRequest {
        SourceReadRequest {
            repository_root: workspace.path().display().to_string(),
            path: path.to_string(),
            expected_content_hash: ContentHash::new(format!("sha256:{}", sha256_hex(bytes)))
                .expect("valid hash"),
            max_file_bytes: 1024,
        }
    }

    #[test]
    fn reads_utf8_source_after_hash_validation() {
        let workspace = TempWorkspace::new("source-store-read");
        let source = b"export const a = 1;\n";
        fs::write(workspace.path().join("a.ts"), source).expect("write source");

        let result = FilesystemSourceStore
            .read_source(request(&workspace, "a.ts", source))
            .expect("read source");

        assert_eq!(result.path, "a.ts");
        assert_eq!(result.text, "export const a = 1;\n");
    }

    #[test]
    fn rejects_traversal_absolute_symlink_hash_mismatch_and_non_utf8() {
        let workspace = TempWorkspace::new("source-store-rejects");
        fs::write(workspace.path().join("a.ts"), b"export const a = 1;\n").expect("write source");
        fs::write(workspace.path().join("binary.ts"), [0xff, 0xfe]).expect("write binary");
        #[cfg(unix)]
        std::os::unix::fs::symlink(
            workspace.path().join("a.ts"),
            workspace.path().join("link.ts"),
        )
        .expect("create symlink");

        for bad_path in ["", "../a.ts", "/tmp/a.ts", "C:\\tmp\\a.ts"] {
            let error = FilesystemSourceStore
                .read_source(request(&workspace, bad_path, b""))
                .expect_err("invalid path must fail");
            assert!(matches!(error, SourceStoreError::InvalidRequest(_)));
        }

        let mut stale = request(&workspace, "a.ts", b"different");
        stale.expected_content_hash = ContentHash::new(
            "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        )
        .expect("valid hash");
        let error = FilesystemSourceStore
            .read_source(stale)
            .expect_err("hash mismatch must fail");
        assert!(matches!(error, SourceStoreError::HashMismatch(_)));

        let error = FilesystemSourceStore
            .read_source(request(&workspace, "binary.ts", &[0xff, 0xfe]))
            .expect_err("non-utf8 source must fail");
        assert!(matches!(error, SourceStoreError::NonUtf8(_)));

        #[cfg(unix)]
        {
            let error = FilesystemSourceStore
                .read_source(request(&workspace, "link.ts", b"export const a = 1;\n"))
                .expect_err("symlink must fail");
            assert!(matches!(error, SourceStoreError::InvalidRequest(_)));
        }
    }
}
