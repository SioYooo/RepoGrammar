//! Local telemetry adapter placeholder.

use crate::ports::telemetry::{TelemetryEvent, TelemetrySink};

#[derive(Debug, Default)]
pub struct LocalTelemetrySink;

impl TelemetrySink for LocalTelemetrySink {
    fn record(&self, _event: TelemetryEvent) {}
}
