//! Measurement taxonomy for product and research metrics.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeasurementKind {
    Measured,
    Derived,
    Estimated,
    CausalExperiment,
}

impl MeasurementKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Measured => "MEASURED",
            Self::Derived => "DERIVED",
            Self::Estimated => "ESTIMATED",
            Self::CausalExperiment => "CAUSAL_EXPERIMENT",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetricReport {
    pub name: String,
    pub kind: MeasurementKind,
    pub value: String,
    pub measurement_source: String,
    pub caveat: Option<String>,
}

impl MetricReport {
    pub fn context_compression_ratio(
        returned_context_units: u64,
        eligible_family_source_units: u64,
    ) -> Result<Self, String> {
        if eligible_family_source_units == 0 {
            return Err("eligible family source must be greater than zero".to_string());
        }

        Ok(Self {
            name: "context_compression_ratio".to_string(),
            kind: MeasurementKind::Derived,
            value: format!("{returned_context_units}/{eligible_family_source_units}"),
            measurement_source: "returned context units over eligible family source units"
                .to_string(),
            caveat: Some("derived context ratio, not actual token savings".to_string()),
        })
    }

    pub fn actual_token_savings(
        baseline_session_tokens: u64,
        treatment_session_tokens: u64,
        tokenizer_or_host_source: impl Into<String>,
    ) -> Result<Self, String> {
        let source = tokenizer_or_host_source.into();
        if source.trim().is_empty() {
            return Err("token measurement source is required".to_string());
        }
        let savings = baseline_session_tokens as i128 - treatment_session_tokens as i128;

        Ok(Self {
            name: "token_savings".to_string(),
            kind: MeasurementKind::Derived,
            value: savings.to_string(),
            measurement_source: source,
            caveat: Some(
                "requires comparable baseline and treatment session token counts".to_string(),
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_compression_ratio_is_derived_not_token_savings() {
        let metric = MetricReport::context_compression_ratio(10, 40).expect("valid ratio");

        assert_eq!(metric.kind, MeasurementKind::Derived);
        assert!(metric
            .caveat
            .as_deref()
            .expect("caveat")
            .contains("not actual token savings"));
    }

    #[test]
    fn token_savings_requires_measurement_source() {
        assert!(MetricReport::actual_token_savings(100, 80, "").is_err());
        assert_eq!(
            MetricReport::actual_token_savings(100, 80, "host-counter")
                .expect("valid metric")
                .value,
            "20"
        );
    }
}
