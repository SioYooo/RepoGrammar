//! Bounded lexical candidates for one assembly profile.
//!
//! The profile is GNU `as` 2.46, x86-64, ELF, AT&T syntax and lowercase `.s`
//! inputs only. This scanner never invokes an assembler, preprocessor, linker,
//! debugger, emulator, or program. It does not validate instructions or prove
//! the input's target profile; every file therefore retains a typed profile
//! `UNKNOWN`. Labels, selected directives, and direct call/jump spellings are
//! structural candidates only and can never support a family claim.

use super::{ir_edges_for_units, ir_nodes_for_units, sort_anchor_facts};
use crate::core::model::{
    CodeUnit, CodeUnitId, CodeUnitKind, Evidence, FactCertainty, FactOrigin, Language, Provenance,
    SemanticFact, SemanticFactKind, SourceRange, SymbolId, UnknownReasonCode,
};
use crate::ports::file_discovery::DEFAULT_MAX_FILE_BYTES;
use crate::ports::parser::{
    ParseError, ParseReport, ParserProjectContext, SourceDocument, SourceParseOutput, SourceParser,
};

const ASSEMBLY_ENGINE: &str = "repogrammar-gas-lexical-scanner";
const ASSEMBLY_METHOD: &str = "bounded_gas_2_46_x86_64_elf_att_candidates_v1";
const MAX_LINES: usize = 100_000;
const MAX_LINE_BYTES: usize = 16 * 1024;
const MAX_LABEL_UNITS: usize = 8_192;
const MAX_FACTS: usize = 16_384;
const MAX_POST_SCAN_UNKNOWNS: usize = 5;

#[derive(Debug, Default)]
pub struct AssemblySyntaxParser;

impl SourceParser for AssemblySyntaxParser {
    fn parse(&self, document: SourceDocument<'_>) -> Result<ParseReport, ParseError> {
        parse_output(document).map(|output| output.report)
    }

    fn parse_with_context(
        &self,
        document: SourceDocument<'_>,
        _context: &ParserProjectContext,
    ) -> Result<ParseReport, ParseError> {
        self.parse(document)
    }

    fn parse_with_context_output(
        &self,
        document: SourceDocument<'_>,
        _context: &ParserProjectContext,
    ) -> Result<SourceParseOutput, ParseError> {
        parse_output(document)
    }
}

fn parse_output(document: SourceDocument<'_>) -> Result<SourceParseOutput, ParseError> {
    if document.language != Language::Assembly || !document.path.ends_with(".s") {
        return Err(ParseError::UnsupportedLanguage);
    }
    let full_range = SourceRange::new(0, document.text.len()).map_err(ParseError::Internal)?;
    let provenance = Provenance::new(
        document.path,
        document.content_hash.clone(),
        document.repository_revision.clone(),
    )
    .map_err(ParseError::Internal)?;
    let module = CodeUnit {
        id: CodeUnitId::new(format!(
            "unit:{}#assembly_module:0-{}:0",
            document.path,
            document.text.len()
        ))
        .map_err(ParseError::Internal)?,
        language: Language::Assembly,
        kind: CodeUnitKind::Module,
        range: full_range.clone(),
        provenance: provenance.clone(),
    };
    let mut units = vec![module.clone()];
    let mut facts = vec![unknown_fact(
        &module,
        UnknownReasonCode::MissingProjectConfig,
        "unproven_target_profile",
        full_range.clone(),
        "lowercase .s path does not prove GNU-as 2.46 x86-64 ELF AT&T target selection",
    )?];

    if document.text.len() > usize::try_from(DEFAULT_MAX_FILE_BYTES).unwrap_or(usize::MAX) {
        facts.push(unknown_fact(
            &module,
            UnknownReasonCode::InsufficientSupport,
            "source_byte_limit",
            full_range,
            "assembly source exceeded the bounded input-byte limit",
        )?);
        return finish(units, facts);
    }

    let mut offset = 0usize;
    let mut label_count = 0usize;
    let mut fact_limit_hit = false;
    let mut line_limit_hit = false;
    let mut label_limit_hit = false;
    let mut dialect_conflict = false;
    let mut macro_seen = false;
    let mut conditional_seen = false;
    let mut include_seen = false;

    for (line_index, raw_line) in document.text.split_inclusive('\n').enumerate() {
        let line_end = offset.saturating_add(raw_line.len());
        if line_index >= MAX_LINES || raw_line.len() > MAX_LINE_BYTES {
            line_limit_hit = true;
            break;
        }
        let line_range = SourceRange::new(offset, line_end).map_err(ParseError::Internal)?;
        let line = raw_line.strip_suffix('\n').unwrap_or(raw_line);
        let line = line.strip_suffix('\r').unwrap_or(line);
        let line = strip_hash_comment(line).trim();
        if line.is_empty() {
            offset = line_end;
            continue;
        }

        let (label, remainder) = split_label(line);
        if label.is_some() {
            if label_count == MAX_LABEL_UNITS {
                label_limit_hit = true;
            } else {
                let label_unit = CodeUnit {
                    id: CodeUnitId::new(format!(
                        "unit:{}#assembly_label:{}-{}:{}",
                        document.path, offset, line_end, label_count
                    ))
                    .map_err(ParseError::Internal)?,
                    language: Language::Assembly,
                    kind: CodeUnitKind::Unknown,
                    range: line_range.clone(),
                    provenance: provenance.clone(),
                };
                if facts.len() < MAX_FACTS - MAX_POST_SCAN_UNKNOWNS {
                    facts.push(structural_fact(
                        &label_unit,
                        "assembly.label_candidate",
                        "assembly_candidate=label",
                        "bounded source-visible GNU-as label candidate; symbol identity is not resolved",
                    )?);
                } else {
                    fact_limit_hit = true;
                }
                units.push(label_unit);
                label_count += 1;
            }
        }
        let statement = remainder.trim();
        if statement.is_empty() {
            offset = line_end;
            continue;
        }

        if let Some(directive) = directive_name(statement) {
            match directive {
                ".intel_syntax" | ".code16" | ".code32" => dialect_conflict = true,
                ".include" | ".incbin" => include_seen = true,
                ".macro" | ".endm" | ".irp" | ".irpc" | ".rept" | ".endr" => {
                    macro_seen = true;
                }
                ".if" | ".ifdef" | ".ifndef" | ".ifnotdef" | ".elseif" | ".else" | ".endif" => {
                    conditional_seen = true
                }
                _ => {}
            }
            if is_selected_directive(directive) {
                push_fact_bounded(
                    &mut facts,
                    structural_fact_range(
                        &module,
                        "assembly.directive_candidate",
                        "assembly_candidate=directive",
                        line_range.clone(),
                        "bounded source-visible GNU-as directive candidate; effect is not evaluated",
                    )?,
                    &mut fact_limit_hit,
                );
            }
        } else if !dialect_conflict && direct_branch_candidate(statement) {
            push_fact_bounded(
                &mut facts,
                structural_fact_range(
                    &module,
                    "assembly.direct_branch_candidate",
                    "assembly_candidate=direct_call_or_jump",
                    line_range,
                    "bounded direct call/jump spelling candidate; control-flow target is not resolved",
                )?,
                &mut fact_limit_hit,
            );
        }
        offset = line_end;
    }

    for (reason, kind, note) in [
        (
            line_limit_hit || fact_limit_hit || label_limit_hit,
            "scanner_resource_limit",
            "assembly scanner exceeded a bounded line, fact, or label limit",
        ),
        (
            dialect_conflict,
            "conflicting_dialect_directive",
            "assembly source selected Intel or non-x86-64 code mode outside the AT&T x86-64 lane",
        ),
        (
            macro_seen,
            "macro_expansion",
            "assembly macro or repetition directives were not expanded",
        ),
        (
            conditional_seen,
            "conditional_assembly",
            "conditional assembly directives were not evaluated",
        ),
        (
            include_seen,
            "include_resolution",
            "assembly include/incbin content and search paths were not read or resolved",
        ),
    ] {
        if reason {
            let unknown_reason = match kind {
                "conflicting_dialect_directive" => UnknownReasonCode::ConflictingFacts,
                "conditional_assembly" => UnknownReasonCode::BuildVariantAmbiguity,
                "macro_expansion" | "include_resolution" => UnknownReasonCode::MacroOrPreprocessor,
                _ => UnknownReasonCode::InsufficientSupport,
            };
            facts.push(unknown_fact(
                &module,
                unknown_reason,
                kind,
                module.range.clone(),
                note,
            )?);
        }
    }
    finish(units, facts)
}

fn finish(
    mut units: Vec<CodeUnit>,
    mut facts: Vec<SemanticFact>,
) -> Result<SourceParseOutput, ParseError> {
    units.sort_by(|left, right| {
        (left.range.start_byte, left.range.end_byte, left.id.as_str()).cmp(&(
            right.range.start_byte,
            right.range.end_byte,
            right.id.as_str(),
        ))
    });
    sort_anchor_facts(&mut facts);
    let ir_nodes = ir_nodes_for_units(&units).map_err(ParseError::Internal)?;
    let ir_edges = ir_edges_for_units(&units).map_err(ParseError::Internal)?;
    Ok(SourceParseOutput {
        report: ParseReport {
            units,
            ir_nodes,
            ir_edges,
            semantic_facts: facts,
            diagnostics: Vec::new(),
        },
        python_interface_hash: None,
        dependencies: Vec::new(),
    })
}

fn strip_hash_comment(line: &str) -> &str {
    line.split_once('#').map_or(line, |(before, _)| before)
}

fn split_label(line: &str) -> (Option<&str>, &str) {
    let Some((candidate, remainder)) = line.split_once(':') else {
        return (None, line);
    };
    let candidate = candidate.trim();
    if is_symbol(candidate) {
        (Some(candidate), remainder)
    } else {
        (None, line)
    }
}

fn directive_name(statement: &str) -> Option<&str> {
    let token = statement.split_ascii_whitespace().next()?;
    token.starts_with('.').then_some(token)
}

fn is_selected_directive(directive: &str) -> bool {
    matches!(
        directive,
        ".text"
            | ".data"
            | ".bss"
            | ".section"
            | ".pushsection"
            | ".popsection"
            | ".previous"
            | ".globl"
            | ".global"
            | ".local"
            | ".weak"
            | ".hidden"
            | ".internal"
            | ".protected"
            | ".type"
            | ".size"
            | ".include"
            | ".incbin"
            | ".macro"
            | ".endm"
            | ".irp"
            | ".irpc"
            | ".rept"
            | ".endr"
            | ".if"
            | ".ifdef"
            | ".ifndef"
            | ".ifnotdef"
            | ".elseif"
            | ".else"
            | ".endif"
            | ".att_syntax"
            | ".intel_syntax"
            | ".code16"
            | ".code32"
            | ".code64"
    )
}

fn direct_branch_candidate(statement: &str) -> bool {
    let mut tokens = statement.split_ascii_whitespace();
    let Some(mnemonic) = tokens.next() else {
        return false;
    };
    let is_branch = matches!(mnemonic, "call" | "callq" | "jmp" | "jmpq")
        || (mnemonic.starts_with('j')
            && mnemonic.len() >= 2
            && mnemonic.len() <= 8
            && mnemonic.bytes().all(|byte| byte.is_ascii_lowercase()));
    if !is_branch {
        return false;
    }
    let Some(operand) = tokens.next() else {
        return false;
    };
    tokens.next().is_none()
        && !operand.starts_with('*')
        && !operand.starts_with('%')
        && !operand.starts_with('$')
        && is_symbol(operand.trim_end_matches(','))
}

fn is_symbol(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && value.is_ascii()
        && value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_alphabetic() || matches!(byte, b'_' | b'.'))
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'$' | b'@'))
}

fn push_fact_bounded(facts: &mut Vec<SemanticFact>, fact: SemanticFact, limit_hit: &mut bool) {
    if facts.len() < MAX_FACTS - MAX_POST_SCAN_UNKNOWNS {
        facts.push(fact);
    } else {
        *limit_hit = true;
    }
}

fn structural_fact(
    unit: &CodeUnit,
    target: &str,
    assumption: &str,
    note: &str,
) -> Result<SemanticFact, ParseError> {
    structural_fact_range(unit, target, assumption, unit.range.clone(), note)
}

fn structural_fact_range(
    unit: &CodeUnit,
    target: &str,
    assumption: &str,
    range: SourceRange,
    note: &str,
) -> Result<SemanticFact, ParseError> {
    Ok(SemanticFact {
        kind: SemanticFactKind::Symbol,
        subject: unit.id.as_str().to_string(),
        target: Some(SymbolId::new(target).map_err(ParseError::Internal)?),
        origin: origin(),
        certainty: FactCertainty::Structural,
        evidence: Evidence::new(unit.id.clone(), range, unit.provenance.clone(), note)
            .map_err(ParseError::Internal)?,
        assumptions: vec![assumption.to_string()],
    })
}

fn unknown_fact(
    unit: &CodeUnit,
    reason: UnknownReasonCode,
    kind: &str,
    range: SourceRange,
    note: &str,
) -> Result<SemanticFact, ParseError> {
    Ok(SemanticFact {
        kind: SemanticFactKind::Unknown,
        subject: unit.id.as_str().to_string(),
        target: Some(SymbolId::new(reason.as_protocol_str()).map_err(ParseError::Internal)?),
        origin: origin(),
        certainty: FactCertainty::Unknown,
        evidence: Evidence::new(unit.id.clone(), range, unit.provenance.clone(), note)
            .map_err(ParseError::Internal)?,
        assumptions: vec![
            "affected_claim=assembly_structural_inventory".to_string(),
            format!("assembly_unknown_kind={kind}"),
        ],
    })
}

fn origin() -> FactOrigin {
    FactOrigin {
        engine: ASSEMBLY_ENGINE.to_string(),
        engine_version: env!("CARGO_PKG_VERSION").to_string(),
        method: ASSEMBLY_METHOD.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::{ContentHash, RepositoryRevision};

    fn output(text: &str) -> SourceParseOutput {
        AssemblySyntaxParser
            .parse_with_context_output(
                SourceDocument {
                    path: "src/start.s",
                    language: Language::Assembly,
                    content_hash: ContentHash::new(
                        "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                    )
                    .expect("hash"),
                    repository_revision: RepositoryRevision::new("UNKNOWN").expect("revision"),
                    text,
                },
                &ParserProjectContext::default(),
            )
            .expect("parse assembly candidates")
    }

    fn count_target(output: &SourceParseOutput, target: &str) -> usize {
        output
            .report
            .semantic_facts
            .iter()
            .filter(|fact| fact.target.as_ref().map(SymbolId::as_str) == Some(target))
            .count()
    }

    #[test]
    fn emits_source_visible_labels_directives_and_direct_branches_only() {
        let parsed = output(
            ".text\n.globl entry\n.type entry,@function\nentry:\n  call helper\n  jne .Ldone\n  call *%rax\n.Ldone:\n  ret\n",
        );
        assert_eq!(count_target(&parsed, "assembly.label_candidate"), 2);
        assert_eq!(count_target(&parsed, "assembly.directive_candidate"), 3);
        assert_eq!(count_target(&parsed, "assembly.direct_branch_candidate"), 2);
        assert_eq!(parsed.report.units.len(), 3);
        assert!(parsed.report.semantic_facts.iter().any(|fact| {
            fact.target.as_ref().map(SymbolId::as_str) == Some("MissingProjectConfig")
        }));
        assert!(parsed
            .report
            .semantic_facts
            .iter()
            .all(|fact| !fact.certainty.supports_family_membership()));
    }

    #[test]
    fn conflicting_dialect_suppresses_branch_candidates_and_stays_unknown() {
        let parsed = output(".intel_syntax noprefix\nentry:\n call helper\n");
        assert_eq!(count_target(&parsed, "assembly.direct_branch_candidate"), 0);
        assert!(parsed.report.semantic_facts.iter().any(|fact| {
            fact.target.as_ref().map(SymbolId::as_str) == Some("ConflictingFacts")
        }));
    }

    #[test]
    fn macro_include_and_conditionals_have_separate_typed_unknowns() {
        let parsed = output(
            ".include \"generated.inc\"\n.macro WRAP target\n call \\target\n.endm\n.if FEATURE\n call hidden\n.endif\n",
        );
        for kind in [
            "assembly_unknown_kind=include_resolution",
            "assembly_unknown_kind=macro_expansion",
            "assembly_unknown_kind=conditional_assembly",
        ] {
            assert!(parsed
                .report
                .semantic_facts
                .iter()
                .any(|fact| { fact.assumptions.iter().any(|assumption| assumption == kind) }));
        }
        let debug = format!("{parsed:?}");
        assert!(!debug.contains("generated.inc"));
        assert!(!debug.contains("FEATURE"));
        assert!(!debug.contains("hidden"));
    }

    #[test]
    fn comments_and_indirect_or_malformed_operands_do_not_create_candidates() {
        let parsed = output(
            "# call commented\n call *target\n call %rax\n call $1\n call target extra\n 1:\n bad-label!:\n",
        );
        assert_eq!(count_target(&parsed, "assembly.direct_branch_candidate"), 0);
        assert_eq!(count_target(&parsed, "assembly.label_candidate"), 0);
    }

    #[test]
    fn line_and_label_resource_limits_are_bounded() {
        let overlong = format!("{}\n", "x".repeat(MAX_LINE_BYTES + 1));
        let parsed = output(&overlong);
        assert!(parsed.report.semantic_facts.iter().any(|fact| {
            fact.assumptions
                .iter()
                .any(|value| value == "assembly_unknown_kind=scanner_resource_limit")
        }));

        let labels = (0..=MAX_LABEL_UNITS)
            .map(|index| format!("label_{index}:\n"))
            .collect::<String>();
        let parsed = output(&labels);
        assert_eq!(parsed.report.units.len(), MAX_LABEL_UNITS + 1);
        assert!(parsed.report.semantic_facts.iter().any(|fact| {
            fact.assumptions
                .iter()
                .any(|value| value == "assembly_unknown_kind=scanner_resource_limit")
        }));
        assert!(parsed.report.semantic_facts.len() <= MAX_FACTS);
    }

    #[test]
    fn total_fact_limit_reserves_space_for_typed_abstention() {
        let directives = ".text\n".repeat(MAX_FACTS);
        let parsed = output(&directives);
        assert_eq!(parsed.report.semantic_facts.len(), MAX_FACTS - 4);
        assert!(parsed.report.semantic_facts.iter().any(|fact| {
            fact.assumptions
                .iter()
                .any(|value| value == "assembly_unknown_kind=scanner_resource_limit")
        }));
    }

    #[test]
    fn parser_rejects_other_languages_and_preprocessed_suffix() {
        let parser = AssemblySyntaxParser;
        let document = SourceDocument {
            path: "main.S",
            language: Language::Assembly,
            content_hash: ContentHash::new(
                "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            )
            .expect("hash"),
            repository_revision: RepositoryRevision::new("UNKNOWN").expect("revision"),
            text: ".text\n",
        };
        assert_eq!(
            parser.parse(document.clone()),
            Err(ParseError::UnsupportedLanguage)
        );
        assert_eq!(
            parser.parse(SourceDocument {
                path: "main.s",
                language: Language::C,
                ..document
            }),
            Err(ParseError::UnsupportedLanguage)
        );
    }
}
