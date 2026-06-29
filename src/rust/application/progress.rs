//! Typed progress events independent of terminal rendering.

use serde_json::json;

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
    Known(KnownWorkUnits),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KnownWorkUnits {
    completed: u64,
    total: u64,
}

impl WorkUnits {
    pub fn known(completed: u64, total: u64) -> Result<Self, String> {
        if completed > total {
            Err("completed work units must not exceed total work units".to_string())
        } else {
            Ok(Self::Known(KnownWorkUnits { completed, total }))
        }
    }
}

impl KnownWorkUnits {
    pub fn completed(self) -> u64 {
        self.completed
    }

    pub fn total(self) -> u64 {
        self.total
    }

    pub fn percent(self) -> u64 {
        if self.total == 0 {
            return 100;
        }
        (((self.completed as u128) * 100) / (self.total as u128)).min(100) as u64
    }
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
            WorkUnits::Known(work) => format!(
                "{}: {} ({percent}% {completed}/{total})\n",
                self.stage.as_str(),
                self.message,
                percent = work.percent(),
                completed = work.completed(),
                total = work.total()
            ),
        }
    }

    pub fn render_ndjson(&self) -> String {
        let work = match self.work {
            WorkUnits::Unknown => json!({"kind": "unknown"}),
            WorkUnits::Known(work) => json!({
                "kind": "known",
                "completed": work.completed(),
                "total": work.total(),
                "percent": work.percent(),
            }),
        };
        let mut output = json!({
            "stage": self.stage.as_str(),
            "message": self.message,
            "work": work,
        })
        .to_string();
        output.push('\n');
        output
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
    fn progress_rendering_uses_counts_percentages_and_no_etas() {
        let event = ProgressEvent::new(
            ProgressStage::FileScanning,
            "scanning files",
            WorkUnits::known(3, 10).expect("valid known work units"),
        );

        let plain = event.render_plain();
        assert!(plain.contains("30%"));
        assert!(plain.contains("3/10"));
        assert!(!plain.to_ascii_lowercase().contains("eta"));
    }

    #[test]
    fn known_work_units_reject_completed_above_total() {
        let known = WorkUnits::known(3, 10).expect("valid known work units");
        let WorkUnits::Known(work) = known else {
            panic!("expected known work units");
        };
        assert_eq!(work.completed(), 3);
        assert_eq!(work.total(), 10);
        assert_eq!(work.percent(), 30);
        assert!(WorkUnits::known(11, 10).is_err());
    }

    #[test]
    fn ndjson_unknown_work_is_typed() {
        let event = ProgressEvent::new(
            ProgressStage::ProjectDiscovery,
            "discovering",
            WorkUnits::Unknown,
        );

        let value: serde_json::Value =
            serde_json::from_str(event.render_ndjson().trim()).expect("progress JSON");
        assert_eq!(value["work"]["kind"], "unknown");
    }

    #[test]
    fn progress_schema_lists_every_initialization_stage() {
        let schema = include_str!("../../protocol/progress-event.schema.json");

        for stage in initialization_stages() {
            assert!(schema.contains(stage.as_str()));
        }
    }

    #[test]
    fn ndjson_rendering_escapes_all_json_control_characters() {
        let event = ProgressEvent::new(
            ProgressStage::ProjectDiscovery,
            "nul:\u{0000} backspace:\u{0008}",
            WorkUnits::Unknown,
        );
        let rendered = event.render_ndjson();
        let value: serde_json::Value =
            serde_json::from_str(rendered.trim()).expect("progress JSON");

        assert_eq!(value["message"], "nul:\u{0000} backspace:\u{0008}");
        assert!(!rendered.contains('\u{0000}'));
        assert!(!rendered.contains('\u{0008}'));
    }
}
