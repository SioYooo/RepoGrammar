//! Optional semantic provider capability model.
//!
//! RepoGrammar's baseline analysis is provider-independent: it works with no
//! external analyzer and reports provider-dependent gaps as typed `UNKNOWN`s.
//! Optional language-native providers (a TypeScript program/checker worker, a
//! Python type checker, rust-analyzer, …) can *accelerate* resolution when they
//! are integrated and configured, but they are never required and never fatal.
//!
//! This module is the honest capability registry for those optional providers.
//! It does not execute any analyzer; it only names the known provider slots, the
//! required-mechanism buckets each would resolve, and their current availability.
//! Availability distinguishes three source-free states so an agent can see
//! whether a provider is present, merely unconfigured, or not yet integrated —
//! rather than conflating "optional accelerator absent" with "product broken".

/// A known optional semantic provider slot. This is a fixed, closed vocabulary;
/// it is not evidence and carries no repository-specific text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SemanticProviderSlot {
    /// TypeScript `Program`/`TypeChecker` worker (module/export/type resolution).
    TypeScriptCompiler,
    /// Python type checker (Pyrefly/Pyright) for framework/type identity and the
    /// third-party import tail. No adapter is wired yet.
    PythonTypeProvider,
    /// rust-analyzer for cross-crate module and trait-dispatch resolution. No
    /// adapter is wired yet.
    RustAnalyzer,
}

impl SemanticProviderSlot {
    pub fn all() -> [Self; 3] {
        [
            Self::TypeScriptCompiler,
            Self::PythonTypeProvider,
            Self::RustAnalyzer,
        ]
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::TypeScriptCompiler => "typescript_compiler",
            Self::PythonTypeProvider => "python_type_provider",
            Self::RustAnalyzer => "rust_analyzer",
        }
    }

    pub fn language(self) -> &'static str {
        match self {
            Self::TypeScriptCompiler => "typescript/javascript",
            Self::PythonTypeProvider => "python",
            Self::RustAnalyzer => "rust",
        }
    }

    /// Whether RepoGrammar has an adapter wired for this slot today. When false,
    /// the slot is documented but not yet integrated, so it is always
    /// [`ProviderAvailability::NotIntegrated`] regardless of configuration.
    pub fn is_integrated(self) -> bool {
        match self {
            Self::TypeScriptCompiler => true,
            Self::PythonTypeProvider | Self::RustAnalyzer => false,
        }
    }

    /// The environment variable that configures an integrated provider, if any.
    pub fn config_env_var(self) -> Option<&'static str> {
        match self {
            Self::TypeScriptCompiler => Some("REPOGRAMMAR_TYPESCRIPT_WORKER"),
            Self::PythonTypeProvider | Self::RustAnalyzer => None,
        }
    }

    /// The host runtime binary required to run RepoGrammar's bundled worker for
    /// this slot, when one ships. `Some("node")` for the checked-in TypeScript
    /// worker; `None` when there is no bundled worker (the provider would rely on
    /// an external analyzer binary instead).
    pub fn bundled_worker_runtime(self) -> Option<&'static str> {
        match self {
            Self::TypeScriptCompiler => Some("node"),
            Self::PythonTypeProvider | Self::RustAnalyzer => None,
        }
    }

    /// The required-mechanism buckets (see the UNKNOWN inventory) this provider
    /// would resolve. These are stable, low-cardinality mechanism codes.
    pub fn resolves_mechanisms(self) -> &'static [&'static str] {
        match self {
            Self::TypeScriptCompiler => &[
                "typescript_module_resolver",
                "typescript_paths_resolver",
                "typescript_package_entry_model",
                "typescript_export_graph",
            ],
            Self::PythonTypeProvider => &[
                "python_import_graph",
                "fastapi_dependency_graph",
                "framework_semantic_provider",
                "resolve_dependency_metadata",
            ],
            Self::RustAnalyzer => &[
                "rust_module_graph",
                "rust_trait_dispatch_model",
                "cargo_feature_cfg_model",
            ],
        }
    }
}

/// The current availability of an optional provider slot. Source-free vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderAvailability {
    /// Integrated and configured/available for use.
    Configured,
    /// Integrated, not configured, but RepoGrammar ships a worker for it and its
    /// runtime is present, so it can be enabled here right now (still opt-in).
    AvailableBundled,
    /// Integrated but not configured and not trivially runnable here.
    NotConfigured,
    /// RepoGrammar has no adapter for this slot yet.
    NotIntegrated,
}

impl ProviderAvailability {
    pub fn as_protocol_str(self) -> &'static str {
        match self {
            Self::Configured => "configured",
            Self::AvailableBundled => "available_bundled",
            Self::NotConfigured => "not_configured",
            Self::NotIntegrated => "not_integrated",
        }
    }

    pub fn parse_protocol_str(value: &str) -> Result<Self, String> {
        match value {
            "configured" => Ok(Self::Configured),
            "available_bundled" => Ok(Self::AvailableBundled),
            "not_configured" => Ok(Self::NotConfigured),
            "not_integrated" => Ok(Self::NotIntegrated),
            _ => Err(format!("unsupported provider availability {value}")),
        }
    }
}

/// Resolve a slot's availability. A slot with no adapter is always
/// `NotIntegrated`. An integrated slot is `Configured` when its config signal is
/// present; otherwise, when RepoGrammar ships a worker for it and that worker's
/// runtime is on the host, it is `AvailableBundled` (enable-able here, still
/// opt-in, never auto-launched); otherwise `NotConfigured`. `runtime_present`
/// must reflect whether [`SemanticProviderSlot::bundled_worker_runtime`] is
/// available on the host.
pub fn provider_availability(
    slot: SemanticProviderSlot,
    config_present: bool,
    runtime_present: bool,
) -> ProviderAvailability {
    if !slot.is_integrated() {
        ProviderAvailability::NotIntegrated
    } else if config_present {
        ProviderAvailability::Configured
    } else if slot.bundled_worker_runtime().is_some() && runtime_present {
        ProviderAvailability::AvailableBundled
    } else {
        ProviderAvailability::NotConfigured
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_slots_and_availability_use_stable_source_free_tokens() {
        for slot in SemanticProviderSlot::all() {
            assert!(slot
                .id()
                .chars()
                .all(|c| c.is_ascii_lowercase() || c == '_'));
            // Documented mechanisms are non-empty and low-cardinality.
            assert!(!slot.resolves_mechanisms().is_empty());
        }
        for (availability, token) in [
            (ProviderAvailability::Configured, "configured"),
            (ProviderAvailability::AvailableBundled, "available_bundled"),
            (ProviderAvailability::NotConfigured, "not_configured"),
            (ProviderAvailability::NotIntegrated, "not_integrated"),
        ] {
            assert_eq!(availability.as_protocol_str(), token);
            assert_eq!(
                ProviderAvailability::parse_protocol_str(token),
                Ok(availability)
            );
        }
        assert!(ProviderAvailability::parse_protocol_str("nope").is_err());
    }

    #[test]
    fn availability_reflects_integration_configuration_and_bundled_runtime() {
        // Integrated slot: configured whenever its config signal is present,
        // regardless of runtime detection.
        assert_eq!(
            provider_availability(SemanticProviderSlot::TypeScriptCompiler, true, false),
            ProviderAvailability::Configured
        );
        // Not configured but the bundled worker's runtime is present here: it can
        // be enabled now (still opt-in, never auto-launched).
        assert_eq!(
            provider_availability(SemanticProviderSlot::TypeScriptCompiler, false, true),
            ProviderAvailability::AvailableBundled
        );
        // Not configured and the runtime is absent: not trivially runnable here.
        assert_eq!(
            provider_availability(SemanticProviderSlot::TypeScriptCompiler, false, false),
            ProviderAvailability::NotConfigured
        );
        // Non-integrated slots stay NotIntegrated regardless of any signal, so a
        // stray env var or present runtime can never falsely claim a provider
        // that does not exist.
        assert_eq!(
            provider_availability(SemanticProviderSlot::PythonTypeProvider, true, true),
            ProviderAvailability::NotIntegrated
        );
        assert_eq!(
            provider_availability(SemanticProviderSlot::RustAnalyzer, true, true),
            ProviderAvailability::NotIntegrated
        );
        // The bundled-worker runtime is only defined for the slot that ships one.
        assert_eq!(
            SemanticProviderSlot::TypeScriptCompiler.bundled_worker_runtime(),
            Some("node")
        );
        assert_eq!(
            SemanticProviderSlot::PythonTypeProvider.bundled_worker_runtime(),
            None
        );
        // The Python worker override configures only the syntax worker, never a
        // type provider, so PythonTypeProvider has no config signal.
        assert_eq!(
            SemanticProviderSlot::PythonTypeProvider.config_env_var(),
            None
        );
    }
}
