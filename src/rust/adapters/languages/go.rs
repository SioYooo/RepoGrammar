//! Go discovery and path-eligibility configuration.
//!
//! This module intentionally does not parse Go source or select a build
//! environment. It records only repository-relative path shape so later Go
//! frontends have one authoritative prefilter instead of reimplementing Go
//! tool exclusions and filename constraints.

use crate::core::policy::paths::validate_repo_relative_path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GoLanguageAdapter;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoPathExclusion {
    DotOrUnderscoreComponent,
    VendorDirectory,
    TestdataDirectory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoPlatformSuffix {
    Goos,
    Goarch,
    GoosGoarch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoPathEligibility {
    NotGoSource,
    Excluded(GoPathExclusion),
    OrdinarySource,
    TestSource,
    PlatformConstrainedSource(GoPlatformSuffix),
    PlatformConstrainedTest(GoPlatformSuffix),
}

// Discovery-only audit snapshot from Go 1.26.5 `internal/syslist.KnownOS` and
// `KnownArch` (https://cs.opensource.google/go/go/+/refs/tags/go1.26.5:src/internal/syslist/syslist.go).
// The version-tagged official Go source
// documents these as past, present, and future values used for filename
// matching. This snapshot does not select a build environment and is not a
// semantic, family, or support authority. Unknown future suffixes remain
// ordinary inventory until this dated list is deliberately re-audited.
const GOOS_VALUES: &[&str] = &[
    "aix",
    "android",
    "darwin",
    "dragonfly",
    "freebsd",
    "hurd",
    "illumos",
    "ios",
    "js",
    "linux",
    "nacl",
    "netbsd",
    "openbsd",
    "plan9",
    "solaris",
    "wasip1",
    "windows",
    "zos",
];

const GOARCH_VALUES: &[&str] = &[
    "386",
    "amd64",
    "amd64p32",
    "arm",
    "armbe",
    "arm64",
    "arm64be",
    "loong64",
    "mips",
    "mipsle",
    "mips64",
    "mips64le",
    "mips64p32",
    "mips64p32le",
    "ppc",
    "ppc64",
    "ppc64le",
    "riscv",
    "riscv64",
    "s390",
    "s390x",
    "sparc",
    "sparc64",
    "wasm",
];

impl GoLanguageAdapter {
    pub fn supports_extension(extension: &str) -> bool {
        extension == "go"
    }

    pub fn is_project_config_path(path: &str) -> bool {
        if validate_repo_relative_path(path).is_err() {
            return false;
        }
        matches!(path.rsplit('/').next(), Some("go.mod" | "go.work"))
    }

    pub fn classify_source_path(path: &str) -> GoPathEligibility {
        if validate_repo_relative_path(path).is_err() {
            return GoPathEligibility::NotGoSource;
        }
        let mut components = path.split('/');
        let Some(file_name) = components.next_back() else {
            return GoPathEligibility::NotGoSource;
        };
        if file_name.is_empty() || !file_name.ends_with(".go") {
            return GoPathEligibility::NotGoSource;
        }

        for component in path.split('/') {
            if component.starts_with('.') || component.starts_with('_') {
                return GoPathEligibility::Excluded(GoPathExclusion::DotOrUnderscoreComponent);
            }
        }
        for component in path.split('/') {
            if component == "vendor" {
                return GoPathEligibility::Excluded(GoPathExclusion::VendorDirectory);
            }
            if component == "testdata" {
                return GoPathEligibility::Excluded(GoPathExclusion::TestdataDirectory);
            }
        }

        let stem = file_name
            .strip_suffix(".go")
            .expect("checked .go suffix above");
        let (stem, is_test) = match stem.strip_suffix("_test") {
            Some(stem) => (stem, true),
            None => (stem, false),
        };
        let platform_suffix = platform_suffix(stem);
        match (is_test, platform_suffix) {
            (false, None) => GoPathEligibility::OrdinarySource,
            (true, None) => GoPathEligibility::TestSource,
            (false, Some(suffix)) => GoPathEligibility::PlatformConstrainedSource(suffix),
            (true, Some(suffix)) => GoPathEligibility::PlatformConstrainedTest(suffix),
        }
    }
}

fn platform_suffix(stem: &str) -> Option<GoPlatformSuffix> {
    let mut parts = stem.rsplit('_');
    let last = parts.next()?;
    let previous = parts.next()?;
    if parts.next().is_some() && GOOS_VALUES.contains(&previous) && GOARCH_VALUES.contains(&last) {
        return Some(GoPlatformSuffix::GoosGoarch);
    }
    if GOOS_VALUES.contains(&last) {
        Some(GoPlatformSuffix::Goos)
    } else if GOARCH_VALUES.contains(&last) {
        Some(GoPlatformSuffix::Goarch)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_source_and_nested_config_paths_exactly() {
        assert!(GoLanguageAdapter::supports_extension("go"));
        assert!(!GoLanguageAdapter::supports_extension("GO"));
        for path in ["go.mod", "go.work", "nested/go.mod", "nested/go.work"] {
            assert!(GoLanguageAdapter::is_project_config_path(path), "{path}");
        }
        for path in ["go.sum", "go.work.sum", "nested/go.mod.bak", "mod.go"] {
            assert!(!GoLanguageAdapter::is_project_config_path(path), "{path}");
        }
        for path in [
            "",
            "/go.mod",
            "C:/go.mod",
            "./go.mod",
            "nested/../go.mod",
            "nested//go.work",
            "nested\\go.mod",
        ] {
            assert!(!GoLanguageAdapter::is_project_config_path(path), "{path:?}");
        }
    }

    #[test]
    fn classifies_go_tool_exclusions_without_ambient_configuration() {
        for path in [
            ".hidden.go",
            "_hidden.go",
            ".hidden/file.go",
            "_gen/file.go",
        ] {
            assert_eq!(
                GoLanguageAdapter::classify_source_path(path),
                GoPathEligibility::Excluded(GoPathExclusion::DotOrUnderscoreComponent),
                "{path}"
            );
        }
        assert_eq!(
            GoLanguageAdapter::classify_source_path("vendor/example.com/lib/file.go"),
            GoPathEligibility::Excluded(GoPathExclusion::VendorDirectory)
        );
        assert_eq!(
            GoLanguageAdapter::classify_source_path("pkg/testdata/fixture.go"),
            GoPathEligibility::Excluded(GoPathExclusion::TestdataDirectory)
        );
    }

    #[test]
    fn distinguishes_test_and_platform_suffix_shape_without_selecting_it() {
        assert_eq!(
            GoLanguageAdapter::classify_source_path("pkg/file.go"),
            GoPathEligibility::OrdinarySource
        );
        assert_eq!(
            GoLanguageAdapter::classify_source_path("pkg/file_test.go"),
            GoPathEligibility::TestSource
        );
        assert_eq!(
            GoLanguageAdapter::classify_source_path("pkg/file_linux.go"),
            GoPathEligibility::PlatformConstrainedSource(GoPlatformSuffix::Goos)
        );
        assert_eq!(
            GoLanguageAdapter::classify_source_path("pkg/file_amd64_test.go"),
            GoPathEligibility::PlatformConstrainedTest(GoPlatformSuffix::Goarch)
        );
        assert_eq!(
            GoLanguageAdapter::classify_source_path("pkg/file_linux_arm64_test.go"),
            GoPathEligibility::PlatformConstrainedTest(GoPlatformSuffix::GoosGoarch)
        );
        assert_eq!(
            GoLanguageAdapter::classify_source_path("pkg/http_client.go"),
            GoPathEligibility::OrdinarySource
        );
        assert_eq!(
            GoLanguageAdapter::classify_source_path("pkg/file_futureos.go"),
            GoPathEligibility::OrdinarySource
        );
        assert_eq!(
            GoLanguageAdapter::classify_source_path("pkg/file.go.bak"),
            GoPathEligibility::NotGoSource
        );
    }

    #[test]
    fn platform_suffix_tail_scan_preserves_component_semantics_without_allocation() {
        for (stem, expected) in [
            ("file", None),
            ("linux", None),
            ("file_linux", Some(GoPlatformSuffix::Goos)),
            ("file_amd64", Some(GoPlatformSuffix::Goarch)),
            ("file_linux_arm64", Some(GoPlatformSuffix::GoosGoarch)),
            ("file_futureos", None),
            ("file__linux", Some(GoPlatformSuffix::Goos)),
            ("_linux", Some(GoPlatformSuffix::Goos)),
        ] {
            assert_eq!(platform_suffix(stem), expected, "{stem}");
        }
    }

    #[test]
    fn rejects_non_normalized_source_paths_and_invalid_separators() {
        for path in [
            "",
            "/pkg/file.go",
            "C:/pkg/file.go",
            "./file.go",
            "pkg/../file.go",
            "pkg//file.go",
            "pkg\\file.go",
            "file://pkg/file.go",
        ] {
            assert_eq!(
                GoLanguageAdapter::classify_source_path(path),
                GoPathEligibility::NotGoSource,
                "{path:?}"
            );
        }
    }
}
