//! Bounded GNU assembler path classification.
//!
//! Only lowercase `.s` is admitted to the static scanner. Uppercase `.S`
//! conventionally requests C-preprocessor handling and is therefore an
//! explicit exclusion. NASM/MASM and other architecture/dialect suffixes are
//! outside this x86-64 ELF GNU-as AT&T candidate lane.

use crate::core::policy::paths::validate_repo_relative_path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AssemblyLanguageAdapter;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssemblyPathClassification {
    NotAssembly,
    Source,
    ExcludedPreprocessedSource,
}

impl AssemblyLanguageAdapter {
    pub fn classify_path(path: &str) -> AssemblyPathClassification {
        if validate_repo_relative_path(path).is_err() {
            return AssemblyPathClassification::NotAssembly;
        }
        let Some(file_name) = path.rsplit('/').next() else {
            return AssemblyPathClassification::NotAssembly;
        };
        if file_name.ends_with(".s") {
            AssemblyPathClassification::Source
        } else if file_name.ends_with(".S") {
            AssemblyPathClassification::ExcludedPreprocessedSource
        } else {
            AssemblyPathClassification::NotAssembly
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admits_only_non_preprocessed_lowercase_gas_sources() {
        for path in ["main.s", "src/startup.s", ".s"] {
            assert_eq!(
                AssemblyLanguageAdapter::classify_path(path),
                AssemblyPathClassification::Source,
                "{path}"
            );
        }
        for path in ["main.S", "src/startup.S"] {
            assert_eq!(
                AssemblyLanguageAdapter::classify_path(path),
                AssemblyPathClassification::ExcludedPreprocessedSource,
                "{path}"
            );
        }
        for path in [
            "main.asm",
            "main.nasm",
            "main.mas",
            "main.inc",
            "main.s.bak",
        ] {
            assert_eq!(
                AssemblyLanguageAdapter::classify_path(path),
                AssemblyPathClassification::NotAssembly,
                "{path}"
            );
        }
    }

    #[test]
    fn rejects_non_normalized_paths() {
        for path in [
            "",
            "/main.s",
            "./main.s",
            "src/../main.s",
            "src//main.s",
            "src\\main.s",
            "C:/main.s",
        ] {
            assert_eq!(
                AssemblyLanguageAdapter::classify_path(path),
                AssemblyPathClassification::NotAssembly,
                "{path:?}"
            );
        }
    }
}
