//! Local telemetry port for diagnostic events.
//!
//! The bootstrap does not define any external telemetry transport.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TelemetryEvent {
    pub name: String,
    pub detail: String,
}

pub trait TelemetrySink {
    fn record(&self, event: TelemetryEvent);
}
