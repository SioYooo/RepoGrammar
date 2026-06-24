//! Shared deterministic helpers for tests.

use std::fs;
use std::path::PathBuf;

#[derive(Debug)]
pub struct TempWorkspace {
    path: PathBuf,
}

impl TempWorkspace {
    pub fn new(prefix: &str) -> Self {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "repogrammar-{prefix}-{}-{}",
            std::process::id(),
            unique_suffix()
        ));
        fs::create_dir_all(&path).expect("create temp workspace");
        Self { path }
    }

    pub fn path(&self) -> &std::path::Path {
        &self.path
    }
}

impl Drop for TempWorkspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn unique_suffix() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time after unix epoch")
        .as_nanos()
}
