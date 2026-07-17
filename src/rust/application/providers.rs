//! Optional semantic provider capability reporting.
//!
//! Produces an honest, source-free snapshot of the optional provider slots and
//! their current availability, derived from configuration signals only. No
//! analyzer is executed. Missing providers are reported as optional accelerators
//! that are absent — never as errors — so the baseline product keeps working.

use crate::core::model::{provider_availability, ProviderAvailability, SemanticProviderSlot};

/// The availability of one optional provider slot at report time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderSlotStatus {
    pub slot: SemanticProviderSlot,
    pub availability: ProviderAvailability,
}

/// Build the optional-provider capability report. `config_lookup` resolves a
/// configuration environment variable to its value and `runtime_available`
/// reports whether a host runtime binary (e.g. `node`) is present, so detection
/// is pure and testable and every environment boundary stays at the caller. A
/// slot is `Configured` only when it is integrated and its config signal is
/// present and non-empty; otherwise, when RepoGrammar ships a worker for it and
/// that worker's runtime is present, it is `AvailableBundled` (enable-able here,
/// still opt-in); non-integrated slots are always `NotIntegrated`.
pub fn optional_provider_report(
    config_lookup: impl Fn(&str) -> Option<String>,
    runtime_available: impl Fn(&str) -> bool,
) -> Vec<ProviderSlotStatus> {
    SemanticProviderSlot::all()
        .into_iter()
        .map(|slot| {
            let config_present = slot
                .config_env_var()
                .and_then(&config_lookup)
                .is_some_and(|value| !value.trim().is_empty());
            let runtime_present = slot
                .bundled_worker_runtime()
                .is_some_and(&runtime_available);
            ProviderSlotStatus {
                slot,
                availability: provider_availability(slot, config_present, runtime_present),
            }
        })
        .collect()
}

/// Recovery code for a provider-resolvable mechanism an *integrated* provider can
/// act on: the provider exists and `doctor` shows how to configure it, so the
/// action is executable.
const ENABLE_PROVIDER_RECOVERY_CODE: &str = "enable_provider";

/// Recovery code for a provider-shaped mechanism that no integrated provider
/// resolves in this version: a registered-but-not-integrated slot's bucket, or a
/// framework/dependency-injection/build model only a future optional provider
/// could resolve. Honest guidance names no provider an agent cannot enable.
const NOT_IMPLEMENTED_RECOVERY_CODE: &str = "not_implemented_in_current_version";

/// Framework, dependency-injection, and build models that no registry slot
/// resolves today but that are provider-shaped: only a future optional semantic
/// provider could resolve them. They must recover via
/// [`NOT_IMPLEMENTED_RECOVERY_CODE`], never via an `enable_provider` action an
/// agent cannot execute because no such provider exists. Kept sorted, disjoint
/// from every [`SemanticProviderSlot`] bucket, and low-cardinality; membership is
/// asserted against the registry in tests so a mechanism cannot be listed here
/// and in a slot at once.
const FUTURE_PROVIDER_MECHANISMS: &[&str] = &[
    "aspnet_route_literal_model",
    "axum_route_model",
    "cpp_compile_commands_model",
    "cpp_test_framework_model",
    "csharp_di_model",
    "dependency_injection_model",
    "django_project_model",
    "django_settings_model",
    "drizzle_db_model",
    "fastify_receiver_model",
    "flask_app_model",
    "hono_receiver_model",
    "java_spring_route_literal_model",
    "java_test_annotation_model",
    "jaxrs_resource_model",
    "jpa_entity_model",
    "nestjs_di_model",
    "prisma_client_model",
    "pydantic_validator_model",
    "spring_component_scan_model",
    "spring_data_repository_model",
    "spring_di_model",
    "sqlalchemy_model_graph",
    "sqlalchemy_session_model",
];

/// Single cross-check authority deciding the provider-related recovery code for a
/// required `mechanism` by consulting the optional-provider registry, or `None`
/// when the mechanism is not provider-shaped and keeps its existing non-provider
/// recovery code. This is the one place that maps a mechanism to a provider
/// action, so recovery guidance can never tell an agent to enable a provider that
/// does not exist; callers must route through it rather than hard-code the
/// decision.
///
/// - A mechanism an *integrated* slot resolves yields
///   [`ENABLE_PROVIDER_RECOVERY_CODE`] — executable because the provider is
///   present and `doctor` shows how to configure it. Today only the TypeScript
///   compiler slot is integrated.
/// - A mechanism a *registered-but-not-integrated* slot would resolve, or a
///   [`FUTURE_PROVIDER_MECHANISMS`] model no slot resolves, yields
///   [`NOT_IMPLEMENTED_RECOVERY_CODE`] — no provider can act on it in this
///   version, so guidance must not promise one.
/// - Any other mechanism is not provider-shaped and yields `None`.
pub fn provider_recovery_code(mechanism: &str) -> Option<&'static str> {
    for slot in SemanticProviderSlot::all() {
        if slot.resolves_mechanisms().contains(&mechanism) {
            return Some(if slot.is_integrated() {
                ENABLE_PROVIDER_RECOVERY_CODE
            } else {
                NOT_IMPLEMENTED_RECOVERY_CODE
            });
        }
    }
    FUTURE_PROVIDER_MECHANISMS
        .contains(&mechanism)
        .then_some(NOT_IMPLEMENTED_RECOVERY_CODE)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};

    fn ts_availability(report: &[ProviderSlotStatus]) -> ProviderAvailability {
        report
            .iter()
            .find(|status| status.slot == SemanticProviderSlot::TypeScriptCompiler)
            .expect("typescript slot present")
            .availability
    }

    #[test]
    fn report_lists_every_slot_and_reflects_configuration() {
        let mut env = BTreeMap::new();
        env.insert(
            "REPOGRAMMAR_TYPESCRIPT_WORKER".to_string(),
            "/path/to/worker".to_string(),
        );
        let report = optional_provider_report(|key| env.get(key).cloned(), |_| true);

        assert_eq!(report.len(), SemanticProviderSlot::all().len());
        // Configured wins over runtime detection.
        assert_eq!(ts_availability(&report), ProviderAvailability::Configured);
        // Non-integrated slots stay NotIntegrated even though nothing configures them.
        for slot in [
            SemanticProviderSlot::PythonTypeProvider,
            SemanticProviderSlot::RustAnalyzer,
        ] {
            let status = report
                .iter()
                .find(|status| status.slot == slot)
                .expect("slot present");
            assert_eq!(status.availability, ProviderAvailability::NotIntegrated);
        }
    }

    #[test]
    fn bundled_worker_runtime_present_makes_it_available_not_fatal() {
        // No env configured, but the bundled worker's runtime (node) is present:
        // TypeScript is AvailableBundled (enable-able here), never an error.
        let report = optional_provider_report(|_| None, |runtime| runtime == "node");
        assert_eq!(
            ts_availability(&report),
            ProviderAvailability::AvailableBundled
        );
    }

    #[test]
    fn without_runtime_or_config_integrated_slot_is_not_configured() {
        // Neither configured nor a present runtime: NotConfigured (not runnable
        // here), still not an error.
        let report = optional_provider_report(|_| None, |_| false);
        assert_eq!(
            ts_availability(&report),
            ProviderAvailability::NotConfigured
        );
    }

    #[test]
    fn blank_configuration_value_is_not_treated_as_configured() {
        let report = optional_provider_report(
            |key| {
                if key == "REPOGRAMMAR_TYPESCRIPT_WORKER" {
                    Some("   ".to_string())
                } else {
                    None
                }
            },
            |_| false,
        );
        assert_eq!(
            ts_availability(&report),
            ProviderAvailability::NotConfigured
        );
    }

    #[test]
    fn integrated_slot_bucket_recovers_via_enable_provider() {
        // The TypeScript compiler is the only integrated slot today, so its
        // resolvable mechanisms guide toward enabling that present provider.
        assert!(SemanticProviderSlot::TypeScriptCompiler.is_integrated());
        for mechanism in SemanticProviderSlot::TypeScriptCompiler.resolves_mechanisms() {
            assert_eq!(
                provider_recovery_code(mechanism),
                Some(ENABLE_PROVIDER_RECOVERY_CODE),
                "{mechanism} is an integrated-provider mechanism"
            );
        }
    }

    #[test]
    fn registered_but_not_integrated_slot_bucket_recovers_via_not_implemented() {
        // Documented-but-unwired slots resolve nothing here, so their mechanisms
        // must not claim an enable-able provider.
        for slot in [
            SemanticProviderSlot::PythonTypeProvider,
            SemanticProviderSlot::RustAnalyzer,
        ] {
            assert!(!slot.is_integrated());
            for mechanism in slot.resolves_mechanisms() {
                assert_eq!(
                    provider_recovery_code(mechanism),
                    Some(NOT_IMPLEMENTED_RECOVERY_CODE),
                    "{mechanism} belongs to a registered-but-not-integrated slot"
                );
            }
        }
    }

    #[test]
    fn future_provider_mechanisms_recover_via_not_implemented() {
        for mechanism in FUTURE_PROVIDER_MECHANISMS {
            assert_eq!(
                provider_recovery_code(mechanism),
                Some(NOT_IMPLEMENTED_RECOVERY_CODE),
                "{mechanism} is a future-provider model with no slot today"
            );
        }
    }

    #[test]
    fn non_provider_mechanisms_are_not_claimed_by_the_authority() {
        // Mechanisms with an executable non-provider recovery keep their code:
        // the authority abstains so the caller's existing mapping wins.
        for mechanism in [
            "source_refresh",
            "project_config_reader",
            "pytest_fixture_graph",
            "typescript_rootdirs_model",
            "typescript_commonjs_alias_model",
            "java_project_graph",
            "csharp_project_model",
            "conflict_resolution",
            "compatible_support_evidence",
            "rust_macro_boundary",
            "cpp_macro_boundary",
            "build_variant_model",
            "runtime_trace_required",
            "spring_proxy_model",
            "import_resolution_provider",
            "unknown",
        ] {
            assert_eq!(
                provider_recovery_code(mechanism),
                None,
                "{mechanism} is not provider-shaped"
            );
        }
    }

    #[test]
    fn authority_recovery_code_set_is_fixed_and_low_cardinality() {
        // The authority emits exactly two provider codes and never anything else.
        let mut codes = BTreeSet::new();
        for slot in SemanticProviderSlot::all() {
            for mechanism in slot.resolves_mechanisms() {
                if let Some(code) = provider_recovery_code(mechanism) {
                    codes.insert(code);
                }
            }
        }
        for mechanism in FUTURE_PROVIDER_MECHANISMS {
            if let Some(code) = provider_recovery_code(mechanism) {
                codes.insert(code);
            }
        }
        assert_eq!(
            codes,
            BTreeSet::from([ENABLE_PROVIDER_RECOVERY_CODE, NOT_IMPLEMENTED_RECOVERY_CODE])
        );

        // The future-provider list stays disjoint from every registry bucket, so a
        // mechanism is never classified twice.
        for mechanism in FUTURE_PROVIDER_MECHANISMS {
            assert!(
                SemanticProviderSlot::all()
                    .iter()
                    .all(|slot| !slot.resolves_mechanisms().contains(mechanism)),
                "{mechanism} is already a registry-slot bucket mechanism"
            );
        }
    }
}
