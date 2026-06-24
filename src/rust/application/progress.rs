//! Typed progress events independent of terminal rendering.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgressStage {
    ProjectDiscovery,
    FileScanning,
    SyntaxParsing,
    SemanticResolution,
    CodeUnitExtractionNormalization,
    CandidateDiscovery,
    FamilyConstruction,
    PersistenceValidation,
}

impl ProgressStage {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ProjectDiscovery => "project_discovery",
            Self::FileScanning => "file_scanning",
            Self::SyntaxParsing => "syntax_parsing",
            Self::SemanticResolution => "semantic_resolution",
            Self::CodeUnitExtractionNormalization => "code_unit_extraction_normalization",
            Self::CandidateDiscovery => "candidate_discovery",
            Self::FamilyConstruction => "family_construction",
            Self::PersistenceValidation => "persistence_validation",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkUnits {
    Unknown,
    Known { completed: u64, total: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgressEvent {
    pub stage: ProgressStage,
    pub message: String,
    pub work: WorkUnits,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgressOutput {
    InteractiveTty,
    Plain,
    Ndjson,
}

impl ProgressEvent {
    pub fn new(stage: ProgressStage, message: impl Into<String>, work: WorkUnits) -> Self {
        Self {
            stage,
            message: message.into(),
            work,
        }
    }

    pub fn render_plain(&self) -> String {
        match self.work {
            WorkUnits::Unknown => format!("{}: {}\n", self.stage.as_str(), self.message),
            WorkUnits::Known { completed, total } => format!(
                "{}: {} ({completed}/{total})\n",
                self.stage.as_str(),
                self.message
            ),
        }
    }

    pub fn render_ndjson(&self) -> String {
        let work = match self.work {
            WorkUnits::Unknown => "\"work\":{\"kind\":\"unknown\"}".to_string(),
            WorkUnits::Known { completed, total } => format!(
                "\"work\":{{\"kind\":\"known\",\"completed\":{completed},\"total\":{total}}}"
            ),
        };
        format!(
            "{{\"stage\":\"{}\",\"message\":\"{}\",{work}}}\n",
            self.stage.as_str(),
            escape_json_string(&self.message)
        )
    }
}

pub fn initialization_stages() -> Vec<ProgressStage> {
    vec![
        ProgressStage::ProjectDiscovery,
        ProgressStage::FileScanning,
        ProgressStage::SyntaxParsing,
        ProgressStage::SemanticResolution,
        ProgressStage::CodeUnitExtractionNormalization,
        ProgressStage::CandidateDiscovery,
        ProgressStage::FamilyConstruction,
        ProgressStage::PersistenceValidation,
    ]
}

fn escape_json_string(value: &str) -> String {
    let mut escaped = String::new();
    for character in value.chars() {
        match character {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            other => escaped.push(other),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initialization_stages_cover_required_pipeline() {
        assert_eq!(
            initialization_stages(),
            vec![
                ProgressStage::ProjectDiscovery,
                ProgressStage::FileScanning,
                ProgressStage::SyntaxParsing,
                ProgressStage::SemanticResolution,
                ProgressStage::CodeUnitExtractionNormalization,
                ProgressStage::CandidateDiscovery,
                ProgressStage::FamilyConstruction,
                ProgressStage::PersistenceValidation,
            ]
        );
    }

    #[test]
    fn progress_rendering_uses_counts_not_percentages_or_etas() {
        let event = ProgressEvent::new(
            ProgressStage::FileScanning,
            "scanning files",
            WorkUnits::Known {
                completed: 3,
                total: 10,
            },
        );

        let plain = event.render_plain();
        assert!(plain.contains("3/10"));
        assert!(!plain.contains('%'));
        assert!(!plain.to_ascii_lowercase().contains("eta"));
    }

    #[test]
    fn ndjson_unknown_work_is_typed() {
        let event = ProgressEvent::new(
            ProgressStage::ProjectDiscovery,
            "discovering",
            WorkUnits::Unknown,
        );

        assert!(event.render_ndjson().contains("\"kind\":\"unknown\""));
    }
}
