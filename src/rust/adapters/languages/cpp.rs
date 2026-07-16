//! C/C++ language adapter configuration.
//!
//! The extension split is deterministic: `.c`/`.h` are parsed with the C
//! grammar (`c` language token) and `.cc`/`.cpp`/`.cxx`/`.hh`/`.hpp`/`.hxx`
//! with the C++ grammar (`cpp` language token). `.h` is treated as C.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CppLanguageAdapter;

const C_EXTENSIONS: &[&str] = &["c", "h"];
const CPP_EXTENSIONS: &[&str] = &["cc", "cpp", "cxx", "hh", "hpp", "hxx"];

impl CppLanguageAdapter {
    pub fn is_c_extension(extension: &str) -> bool {
        C_EXTENSIONS.contains(&extension)
    }

    pub fn is_cpp_extension(extension: &str) -> bool {
        CPP_EXTENSIONS.contains(&extension)
    }

    pub fn supports_extension(extension: &str) -> bool {
        Self::is_c_extension(extension) || Self::is_cpp_extension(extension)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_c_and_cpp_extensions_deterministically() {
        assert!(CppLanguageAdapter::is_c_extension("c"));
        assert!(CppLanguageAdapter::is_c_extension("h"));
        assert!(!CppLanguageAdapter::is_cpp_extension("c"));
        assert!(!CppLanguageAdapter::is_cpp_extension("h"));

        for extension in ["cc", "cpp", "cxx", "hh", "hpp", "hxx"] {
            assert!(CppLanguageAdapter::is_cpp_extension(extension));
            assert!(!CppLanguageAdapter::is_c_extension(extension));
        }

        assert!(CppLanguageAdapter::supports_extension("c"));
        assert!(CppLanguageAdapter::supports_extension("cpp"));
        assert!(!CppLanguageAdapter::supports_extension("cs"));
        assert!(!CppLanguageAdapter::supports_extension("rs"));
    }
}
