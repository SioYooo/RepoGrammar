//! Width-bounded terminal presentation of real, per-stage progress events.

use crate::application::progress::{ProgressEvent, ProgressStage, WorkUnits};

/// Render one terminal line, without carriage return, clearing, or newline.
/// The caller owns TTY detection and honors CI, TERM, NO_COLOR, and locale.
/// Percentages describe this event's stage, never estimated overall progress.
pub fn render_terminal_index_progress_event(
    command: &str,
    event: &ProgressEvent,
    columns: usize,
    unicode: bool,
    color: bool,
) -> String {
    // Reserve the last column to avoid auto-wrapping when the line is rewritten.
    let width = columns.clamp(1, 512).saturating_sub(1);
    if width == 0 {
        return String::new();
    }
    let stage = stage_label(event.stage);
    let counts = match event.work {
        WorkUnits::Known(work) => {
            format!("{}% {}/{}", work.percent(), work.completed(), work.total())
        }
        WorkUnits::Unknown => "working".to_string(),
    };
    let label = format!("{} / {stage}", safe_text(command, width));
    let fixed = label.len() + counts.len() + 4;
    let bar_width = width.saturating_sub(fixed).min(24);
    let mut line = if bar_width >= 4 {
        let bar = match event.work {
            WorkUnits::Known(work) => {
                let filled = if work.total() == 0 {
                    bar_width
                } else {
                    (u128::from(work.completed()) * bar_width as u128 / u128::from(work.total()))
                        as usize
                };
                format!(
                    "{}{}",
                    if unicode { "━" } else { "#" }.repeat(filled),
                    if unicode { "─" } else { "-" }.repeat(bar_width - filled)
                )
            }
            // No clock or fake progress: the marker changes only with real events.
            WorkUnits::Unknown => if unicode { "·" } else { "." }.repeat(bar_width),
        };
        format!("{label} [{bar}] {counts}")
    } else if stage.len() + counts.len() < width {
        format!("{stage} {counts}")
    } else if counts.len() <= width {
        counts
    } else {
        // Exact large counts cannot fit: omit them rather than truncate a number.
        match event.work {
            WorkUnits::Known(work) => format!("{}%", work.percent()),
            WorkUnits::Unknown => "working".to_string(),
        }
    };
    let remaining = width.saturating_sub(line.chars().count());
    if remaining > 4 {
        let message = safe_text(&event.message, remaining - 3);
        if !message.is_empty() {
            line.push_str(" | ");
            line.push_str(&message);
        }
    }
    // Only terminals narrower than even "100%" reach this final truncation.
    if line.chars().count() > width {
        line = if matches!(event.work, WorkUnits::Known(_)) {
            "#".repeat(width)
        } else {
            ".".repeat(width)
        };
    }
    if color {
        format!("\x1b[1;36m{line}\x1b[0m")
    } else {
        line
    }
}

fn stage_label(stage: ProgressStage) -> &'static str {
    match stage {
        ProgressStage::ProjectDiscovery => "Discover",
        ProgressStage::FileScanning => "Scan",
        ProgressStage::SyntaxParsing => "Parse",
        ProgressStage::SemanticResolution => "Resolve",
        ProgressStage::CodeUnitExtractionNormalization => "Normalize",
        ProgressStage::CandidateDiscovery => "Candidates",
        ProgressStage::FamilyConstruction => "Families",
        ProgressStage::PersistenceValidation => "Validate",
    }
}

fn safe_text(text: &str, width: usize) -> String {
    // Messages are diagnostic text, not terminal control sequences. ASCII also
    // keeps display-column accounting correct without a Unicode-width dependency.
    text.chars()
        .take(width)
        .map(|character| {
            if character.is_ascii_graphic() || character == ' ' {
                character
            } else {
                ' '
            }
        })
        .collect::<String>()
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_bar_uses_exact_stage_counts_and_optional_color() {
        let event = ProgressEvent::new(
            ProgressStage::SyntaxParsing,
            "parsed source files",
            WorkUnits::known(3, 4).unwrap(),
        );
        let line = render_terminal_index_progress_event("index", &event, 96, true, false);
        assert!(line.contains("index / Parse"));
        assert!(line.contains("75% 3/4"));
        assert!(line.contains(&"━".repeat(18)));
        assert!(line.contains("parsed source files"));
        assert!(!line.contains(['\r', '\n', '\x1b']));
        let colored = render_terminal_index_progress_event("index", &event, 96, true, true);
        assert_eq!(colored, format!("\x1b[1;36m{line}\x1b[0m"));
        let ascii = render_terminal_index_progress_event("index", &event, 96, false, false);
        assert!(ascii.is_ascii());
        assert!(ascii.contains(&"#".repeat(18)));
    }

    #[test]
    fn unknown_and_narrow_terminal_frames_never_invent_progress_or_wrap() {
        let unknown = ProgressEvent::new(
            ProgressStage::SemanticResolution,
            "waiting\nfor\x1b[31m worker",
            WorkUnits::Unknown,
        );
        let known = ProgressEvent::new(
            ProgressStage::PersistenceValidation,
            "validated",
            WorkUnits::known(u64::MAX - 1, u64::MAX).unwrap(),
        );
        for width in 1..=120 {
            for event in [&unknown, &known] {
                let line = render_terminal_index_progress_event("sync", event, width, true, false);
                assert!(line.chars().count() < width, "width {width}: {line}");
                assert!(!line.contains(['\n', '\r', '\x1b']));
            }
        }
        let line = render_terminal_index_progress_event("sync", &unknown, 80, true, false);
        assert!(line.contains("working"));
        assert!(!line.contains('%'));
        let line = render_terminal_index_progress_event("sync", &known, 120, false, false);
        assert!(line.contains("99% 18446744073709551614/18446744073709551615"));
    }
}
