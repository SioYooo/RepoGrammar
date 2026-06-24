//! Local telemetry port for diagnostic events.
//!
//! Production telemetry must pass through the anonymous allowlist policy before
//! any future network transport is introduced.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TelemetryEvent {
    pub schema_version: u32,
    pub event_kind: String,
    pub command_kind: String,
    pub duration_bucket: Option<String>,
    pub count_bucket: Option<String>,
    pub typed_error_code: Option<String>,
}

pub trait TelemetrySink {
    fn record(&self, event: TelemetryEvent);
}
