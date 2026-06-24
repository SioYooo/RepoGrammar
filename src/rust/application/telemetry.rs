//! Anonymous telemetry and research trace consent policy.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsentDecision {
    Enabled,
    Disabled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TelemetryConsent {
    pub anonymous_product_telemetry: ConsentDecision,
    pub research_trace_collection: ConsentDecision,
}

impl Default for TelemetryConsent {
    fn default() -> Self {
        Self {
            anonymous_product_telemetry: ConsentDecision::Disabled,
            research_trace_collection: ConsentDecision::Disabled,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TelemetryCommand {
    Status,
    On,
    Off,
    Purge,
    ExportLocal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnonymousTelemetrySchema {
    pub version: u32,
    pub allowed_fields: &'static [&'static str],
    pub forbidden_payloads: &'static [&'static str],
}

pub const ANONYMOUS_TELEMETRY_SCHEMA: AnonymousTelemetrySchema = AnonymousTelemetrySchema {
    version: 1,
    allowed_fields: &[
        "schema_version",
        "event_kind",
        "command_kind",
        "duration_bucket",
        "count_bucket",
        "typed_error_code",
    ],
    forbidden_payloads: &[
        "code",
        "path",
        "repository_name",
        "symbol",
        "prompt",
        "query_text",
        "evidence_text",
        "environment_variable",
        "credential",
        "raw_error_message",
    ],
};

pub fn telemetry_disabled_by_environment<F>(lookup: F) -> bool
where
    F: Fn(&str) -> Option<String>,
{
    env_equals(lookup("REPOGRAMMAR_TELEMETRY"), "0") || env_equals(lookup("DO_NOT_TRACK"), "1")
}

fn env_equals(value: Option<String>, expected: &str) -> bool {
    value
        .as_deref()
        .is_some_and(|value| value.trim().eq_ignore_ascii_case(expected))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn telemetry_and_research_consent_are_separate_and_disabled_by_default() {
        let consent = TelemetryConsent::default();

        assert_eq!(
            consent.anonymous_product_telemetry,
            ConsentDecision::Disabled
        );
        assert_eq!(consent.research_trace_collection, ConsentDecision::Disabled);
    }

    #[test]
    fn environment_disables_telemetry() {
        assert!(telemetry_disabled_by_environment(|key| {
            (key == "REPOGRAMMAR_TELEMETRY").then(|| "0".to_string())
        }));
        assert!(telemetry_disabled_by_environment(|key| {
            (key == "DO_NOT_TRACK").then(|| "1".to_string())
        }));
        assert!(!telemetry_disabled_by_environment(|_| None));
    }

    #[test]
    fn anonymous_schema_forbids_sensitive_payloads() {
        assert!(ANONYMOUS_TELEMETRY_SCHEMA
            .forbidden_payloads
            .contains(&"path"));
        assert!(ANONYMOUS_TELEMETRY_SCHEMA
            .forbidden_payloads
            .contains(&"prompt"));
    }
}
