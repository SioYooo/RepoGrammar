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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

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
}
