//! Bounded, non-executing Go project-configuration inventory.
//!
//! This adapter reads repository-owned `go.mod` and `go.work` bytes only. It
//! never invokes the Go toolchain, resolves a module graph, selects a build
//! environment, or turns dependency declarations into language/framework
//! support evidence.

pub(crate) mod source;

use super::{ir_edges_for_units, ir_nodes_for_units};
use crate::core::model::{
    CodeUnit, CodeUnitId, CodeUnitKind, DependencyDirectness, DependencyEcosystem,
    DependencyEvidenceLevel, DependencyRecord, DependencyScope, DependencySnapshot,
    DependencyVersion, Evidence, FactCertainty, FactOrigin, Language, PackageIdentity, Provenance,
    SemanticFact, SemanticFactKind, SourceRange, SymbolId, UnknownReasonCode,
};
use crate::ports::parser::{
    ParseError, ParseReport, ParserProjectContext, SourceDocument, SourceParseOutput, SourceParser,
};
use std::collections::{BTreeMap, BTreeSet};

const GO_CONFIG_ENGINE: &str = "repogrammar-go-project-config";
const GO_CONFIG_METHOD: &str = "bounded_go_module_inventory_v1";
const GO_MOD_MAX_BYTES: usize = 4 * 1024 * 1024;
const GO_MOD_MAX_LINES: usize = 100_000;
const GO_MOD_MAX_LINE_BYTES: usize = 16 * 1024;
const GO_MOD_MAX_TOKENS_PER_LINE: usize = 32;
const GO_MOD_MAX_TOKEN_BYTES: usize = 1_024;
const GO_MOD_MAX_DIRECTIVES: usize = 16_384;
const GO_MOD_MAX_DEPENDENCIES: usize = 2_000;

#[derive(Debug, Default)]
pub struct GoProjectConfigParser;

impl SourceParser for GoProjectConfigParser {
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
    if document.language != Language::GoConfig {
        return Err(ParseError::UnsupportedLanguage);
    }
    let range = SourceRange::new(0, document.text.len()).map_err(ParseError::Internal)?;
    let provenance = Provenance::new(
        document.path,
        document.content_hash.clone(),
        document.repository_revision.clone(),
    )
    .map_err(ParseError::Internal)?;
    let unit = CodeUnit {
        id: CodeUnitId::new(format!(
            "unit:{}#project_config:0-{}:0",
            document.path,
            document.text.len()
        ))
        .map_err(ParseError::Internal)?,
        language: Language::GoConfig,
        kind: CodeUnitKind::ProjectConfig,
        range,
        provenance,
    };
    let basename = document.path.rsplit('/').next().unwrap_or(document.path);
    let (mut facts, dependencies) = match basename {
        "go.mod" => parse_go_mod(document.text, &unit)?,
        "go.work" => parse_go_work(document.text, &unit)?,
        _ => return Err(ParseError::UnsupportedLanguage),
    };
    facts.sort_by(|left, right| {
        (
            left.target.as_ref().map(SymbolId::as_str),
            left.evidence.range.start_byte,
            left.evidence.range.end_byte,
        )
            .cmp(&(
                right.target.as_ref().map(SymbolId::as_str),
                right.evidence.range.start_byte,
                right.evidence.range.end_byte,
            ))
    });
    let units = vec![unit];
    let ir_nodes = ir_nodes_for_units(&units).map_err(ParseError::Internal)?;
    let ir_edges = ir_edges_for_units(&units).map_err(ParseError::Internal)?;
    let dependencies = DependencySnapshot::new(dependencies, Vec::new())
        .map_err(ParseError::Internal)?
        .dependencies;
    Ok(SourceParseOutput {
        report: ParseReport {
            units,
            ir_nodes,
            ir_edges,
            semantic_facts: facts,
            diagnostics: Vec::new(),
        },
        python_interface_hash: None,
        dependencies,
    })
}

fn parse_go_work(
    text: &str,
    unit: &CodeUnit,
) -> Result<(Vec<SemanticFact>, Vec<DependencyRecord>), ParseError> {
    if text.len() > GO_MOD_MAX_BYTES {
        return Ok((
            vec![dependency_unknown(
                unit,
                UnknownReasonCode::InsufficientSupport,
                "workspace_byte_limit",
                unit.range.clone(),
                "Go workspace inventory exceeded the bounded byte limit",
            )?],
            Vec::new(),
        ));
    }
    for (index, raw_line) in text.split_inclusive('\n').enumerate() {
        let line = raw_line.strip_suffix('\n').unwrap_or(raw_line);
        let line = line.strip_suffix('\r').unwrap_or(line);
        if index >= GO_MOD_MAX_LINES || line.len() > GO_MOD_MAX_LINE_BYTES {
            return Ok((
                vec![dependency_unknown(
                    unit,
                    UnknownReasonCode::InsufficientSupport,
                    "workspace_resource_limit",
                    unit.range.clone(),
                    "Go workspace inventory exceeded bounded line limits",
                )?],
                Vec::new(),
            ));
        }
    }
    Ok((
        vec![dependency_unknown(
            unit,
            UnknownReasonCode::BuildVariantAmbiguity,
            "workspace_configuration",
            unit.range.clone(),
            "Go workspace configuration requires unresolved module-graph selection",
        )?],
        Vec::new(),
    ))
}

#[derive(Debug, Clone)]
struct DependencyCandidate {
    module_path: String,
    version: String,
    direct: bool,
    range: SourceRange,
}

#[derive(Debug, Clone, Copy)]
struct UnknownMarker {
    reason: UnknownReasonCode,
    kind: &'static str,
    note: &'static str,
    start_byte: usize,
    end_byte: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BlockDirective {
    Module,
    Require,
    Replace,
    Exclude,
    Retract,
    Tool,
    Godebug,
    Ignore,
}

fn parse_go_mod(
    text: &str,
    unit: &CodeUnit,
) -> Result<(Vec<SemanticFact>, Vec<DependencyRecord>), ParseError> {
    if text.len() > GO_MOD_MAX_BYTES {
        return Ok((
            vec![dependency_unknown(
                unit,
                UnknownReasonCode::InsufficientSupport,
                "manifest_byte_limit",
                unit.range.clone(),
                "Go module dependency inventory exceeded the bounded manifest byte limit",
            )?],
            Vec::new(),
        ));
    }

    let mut candidates = Vec::new();
    let mut markers = Vec::new();
    let mut block = None;
    let mut line_count = 0usize;
    let mut directive_count = 0usize;
    let mut module_directive_count = 0usize;
    let mut go_directive_count = 0usize;
    let mut offset = 0usize;
    let mut dependency_limit_exceeded = false;
    let mut inventory_invalidated = false;

    for raw_line in text.split_inclusive('\n') {
        line_count = line_count.saturating_add(1);
        let line_end = offset.saturating_add(raw_line.len());
        let line = raw_line.strip_suffix('\n').unwrap_or(raw_line);
        let line = line.strip_suffix('\r').unwrap_or(line);
        if line_count > GO_MOD_MAX_LINES || line.len() > GO_MOD_MAX_LINE_BYTES {
            markers.push(UnknownMarker {
                reason: UnknownReasonCode::InsufficientSupport,
                kind: "manifest_resource_limit",
                note: "Go module dependency inventory exceeded bounded line limits",
                start_byte: offset,
                end_byte: line_end,
            });
            inventory_invalidated = true;
            break;
        }
        let lexed = match lex_line(line, offset) {
            Ok(lexed) => lexed,
            Err(LexError::Malformed) => {
                markers.push(UnknownMarker {
                    reason: UnknownReasonCode::MissingProjectConfig,
                    kind: "malformed_go_mod_syntax",
                    note: "Go module dependency inventory contains malformed line syntax",
                    start_byte: offset,
                    end_byte: line_end,
                });
                offset = line_end;
                continue;
            }
            Err(LexError::ResourceLimit) => {
                markers.push(UnknownMarker {
                    reason: UnknownReasonCode::InsufficientSupport,
                    kind: "manifest_token_limit",
                    note: "Go module dependency inventory exceeded bounded token limits",
                    start_byte: offset,
                    end_byte: line_end,
                });
                inventory_invalidated = true;
                break;
            }
        };
        if lexed.tokens.is_empty() {
            offset = line_end;
            continue;
        }
        directive_count = directive_count.saturating_add(1);
        if directive_count > GO_MOD_MAX_DIRECTIVES {
            markers.push(UnknownMarker {
                reason: UnknownReasonCode::InsufficientSupport,
                kind: "directive_limit",
                note: "Go module dependency inventory exceeded the bounded directive limit",
                start_byte: offset,
                end_byte: line_end,
            });
            inventory_invalidated = true;
            break;
        }

        match block {
            Some(_) if is_close_line(&lexed.tokens) => block = None,
            Some(BlockDirective::Module) => {
                if contains_parenthesis(&lexed.tokens)
                    || lexed.tokens.len() != 1
                    || word_at(&lexed.tokens, 0).is_none_or(|path| !is_valid_module_path(path))
                {
                    markers.push(malformed_marker(offset, line_end));
                } else {
                    module_directive_count = module_directive_count.saturating_add(1);
                }
            }
            Some(BlockDirective::Require) => {
                parse_require_line(&lexed, offset, line_end, &mut candidates, &mut markers)
            }
            Some(BlockDirective::Replace) => {
                if contains_parenthesis(&lexed.tokens) {
                    markers.push(malformed_marker(offset, line_end));
                } else {
                    markers.push(replace_marker(&lexed, offset, line_end));
                }
            }
            Some(directive) => {
                if contains_parenthesis(&lexed.tokens)
                    || !valid_unsupported_directive_body(directive, &lexed.tokens)
                {
                    markers.push(malformed_marker(offset, line_end));
                }
            }
            None => {
                if lexed.tokens.len() == 2
                    && unquoted_word_at(&lexed.tokens, 0) == Some("module")
                    && word_at(&lexed.tokens, 1).is_some_and(is_valid_module_path)
                {
                    module_directive_count = module_directive_count.saturating_add(1);
                }
                if lexed.tokens.len() == 2
                    && unquoted_word_at(&lexed.tokens, 0) == Some("go")
                    && unquoted_word_at(&lexed.tokens, 1).is_some_and(is_valid_go_directive)
                {
                    go_directive_count = go_directive_count.saturating_add(1);
                }
                parse_directive_line(
                    &lexed,
                    offset,
                    line_end,
                    &mut block,
                    &mut candidates,
                    &mut markers,
                )
            }
        }
        if candidates.len() > GO_MOD_MAX_DEPENDENCIES {
            dependency_limit_exceeded = true;
            inventory_invalidated = true;
        }
        if markers.len() > 32 {
            deduplicate_markers(&mut markers);
            markers.truncate(32);
        }
        offset = line_end;
    }
    if !text.is_empty() && !text.ends_with('\n') && offset < text.len() {
        // `split_inclusive` still yields the final non-newline-terminated line;
        // this is defensive against future iterator changes.
        markers.push(malformed_marker(offset, text.len()));
    }
    if block.is_some() {
        markers.push(UnknownMarker {
            reason: UnknownReasonCode::MissingProjectConfig,
            kind: "unterminated_directive_block",
            note: "Go module dependency inventory contains an unterminated directive block",
            start_byte: unit.range.start_byte,
            end_byte: unit.range.end_byte,
        });
    }
    if module_directive_count != 1 {
        markers.push(UnknownMarker {
            reason: if module_directive_count == 0 {
                UnknownReasonCode::MissingProjectConfig
            } else {
                UnknownReasonCode::ConflictingFacts
            },
            kind: "missing_or_duplicate_module_directive",
            note: "Go module dependency inventory requires exactly one valid module directive",
            start_byte: unit.range.start_byte,
            end_byte: unit.range.end_byte,
        });
    }
    if go_directive_count > 1 {
        markers.push(UnknownMarker {
            reason: UnknownReasonCode::ConflictingFacts,
            kind: "duplicate_go_directive",
            note: "Go module dependency inventory contains duplicate Go version directives",
            start_byte: unit.range.start_byte,
            end_byte: unit.range.end_byte,
        });
    }
    if inventory_invalidated {
        candidates.clear();
    }
    if dependency_limit_exceeded {
        markers.push(UnknownMarker {
            reason: UnknownReasonCode::InsufficientSupport,
            kind: "dependency_limit",
            note: "Go module dependency inventory exceeded the bounded dependency record limit",
            start_byte: unit.range.start_byte,
            end_byte: unit.range.end_byte,
        });
    }

    if markers.iter().any(|marker| {
        matches!(
            marker.kind,
            "malformed_go_mod_syntax"
                | "invalid_require_boundary"
                | "unterminated_directive_block"
                | "missing_or_duplicate_module_directive"
                | "duplicate_go_directive"
        )
    }) {
        candidates.clear();
    }

    let candidates = remove_conflicting_candidates(candidates, &mut markers);
    deduplicate_markers(&mut markers);
    let mut dependencies = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        dependencies.push(
            DependencyRecord::new(
                PackageIdentity::new(DependencyEcosystem::GoModules, candidate.module_path)
                    .map_err(ParseError::Internal)?,
                Some(DependencyVersion::new(candidate.version).map_err(ParseError::Internal)?),
                None,
                DependencyScope::Unknown,
                false,
                if candidate.direct {
                    DependencyDirectness::Direct
                } else {
                    DependencyDirectness::Transitive
                },
                DependencyEvidenceLevel::ManifestDeclared,
                Evidence::new(
                    unit.id.clone(),
                    candidate.range,
                    unit.provenance.clone(),
                    "bounded go.mod require declaration",
                )
                .map_err(ParseError::Internal)?,
            )
            .map_err(ParseError::Internal)?,
        );
    }
    let mut seen = BTreeSet::new();
    let mut facts = Vec::new();
    for marker in markers {
        if !seen.insert(marker.kind) {
            continue;
        }
        facts.push(dependency_unknown(
            unit,
            marker.reason,
            marker.kind,
            SourceRange::new(marker.start_byte, marker.end_byte).map_err(ParseError::Internal)?,
            marker.note,
        )?);
    }
    Ok((facts, dependencies))
}

fn remove_conflicting_candidates(
    candidates: Vec<DependencyCandidate>,
    markers: &mut Vec<UnknownMarker>,
) -> Vec<DependencyCandidate> {
    let mut grouped = BTreeMap::<String, Vec<DependencyCandidate>>::new();
    for candidate in candidates {
        grouped
            .entry(candidate.module_path.clone())
            .or_default()
            .push(candidate);
    }
    let mut retained = Vec::new();
    for entries in grouped.into_values() {
        if entries.len() == 1 {
            retained.extend(entries);
            continue;
        }
        let start_byte = entries
            .iter()
            .map(|entry| entry.range.start_byte)
            .min()
            .unwrap_or(0);
        let end_byte = entries
            .iter()
            .map(|entry| entry.range.end_byte)
            .max()
            .unwrap_or(start_byte);
        markers.push(UnknownMarker {
            reason: UnknownReasonCode::ConflictingFacts,
            kind: "duplicate_or_conflicting_require",
            note: "Duplicate or conflicting Go module requirements were omitted",
            start_byte,
            end_byte,
        });
    }
    retained
}

fn deduplicate_markers(markers: &mut Vec<UnknownMarker>) {
    let mut seen = BTreeSet::new();
    markers.retain(|marker| seen.insert(marker.kind));
}

fn parse_directive_line(
    lexed: &LexedLine,
    line_start: usize,
    line_end: usize,
    block: &mut Option<BlockDirective>,
    candidates: &mut Vec<DependencyCandidate>,
    markers: &mut Vec<UnknownMarker>,
) {
    let Some(directive) = unquoted_word_at(&lexed.tokens, 0) else {
        markers.push(malformed_marker(line_start, line_end));
        return;
    };
    if lexed.tokens.len() == 2 && matches!(lexed.tokens[1], Token::OpenParen) {
        match directive {
            "module" => *block = Some(BlockDirective::Module),
            "require" => *block = Some(BlockDirective::Require),
            "replace" => {
                markers.push(unsupported_directive_marker(
                    directive, line_start, line_end,
                ));
                *block = Some(BlockDirective::Replace);
            }
            "exclude" | "retract" | "tool" | "godebug" | "ignore" => {
                markers.push(unsupported_directive_marker(
                    directive, line_start, line_end,
                ));
                *block = Some(match directive {
                    "exclude" => BlockDirective::Exclude,
                    "retract" => BlockDirective::Retract,
                    "tool" => BlockDirective::Tool,
                    "godebug" => BlockDirective::Godebug,
                    "ignore" => BlockDirective::Ignore,
                    _ => unreachable!("matched known block directive"),
                });
            }
            _ => markers.push(malformed_marker(line_start, line_end)),
        }
        return;
    }
    if contains_parenthesis(&lexed.tokens) {
        markers.push(malformed_marker(line_start, line_end));
        return;
    }
    match directive {
        "require" => parse_require_line(lexed, line_start, line_end, candidates, markers),
        "module" => {
            if lexed.tokens.len() != 2
                || word_at(&lexed.tokens, 1).is_none_or(|path| !is_valid_module_path(path))
            {
                markers.push(malformed_marker(line_start, line_end));
            }
        }
        "go" => {
            if lexed.tokens.len() != 2
                || unquoted_word_at(&lexed.tokens, 1)
                    .is_none_or(|version| !is_valid_go_directive(version))
            {
                markers.push(malformed_marker(line_start, line_end));
            }
        }
        "replace" => markers.push(replace_marker(lexed, line_start, line_end)),
        "exclude" | "retract" | "tool" | "godebug" | "ignore" => {
            let block_directive = match directive {
                "exclude" => BlockDirective::Exclude,
                "retract" => BlockDirective::Retract,
                "tool" => BlockDirective::Tool,
                "godebug" => BlockDirective::Godebug,
                "ignore" => BlockDirective::Ignore,
                _ => unreachable!("matched known directive"),
            };
            if valid_unsupported_directive_body(block_directive, &lexed.tokens[1..]) {
                markers.push(unsupported_directive_marker(
                    directive, line_start, line_end,
                ));
            } else {
                markers.push(malformed_marker(line_start, line_end));
            }
        }
        "toolchain" => {
            if lexed.tokens.len() == 2
                && unquoted_word_at(&lexed.tokens, 1).is_some_and(is_valid_toolchain_name)
            {
                markers.push(unsupported_directive_marker(
                    directive, line_start, line_end,
                ));
            } else {
                markers.push(malformed_marker(line_start, line_end));
            }
        }
        _ => markers.push(UnknownMarker {
            reason: UnknownReasonCode::InsufficientSupport,
            kind: "unsupported_or_unknown_directive",
            note: "Go module dependency inventory contains an unsupported or unknown directive",
            start_byte: line_start,
            end_byte: line_end,
        }),
    }
}

fn parse_require_line(
    lexed: &LexedLine,
    line_start: usize,
    line_end: usize,
    candidates: &mut Vec<DependencyCandidate>,
    markers: &mut Vec<UnknownMarker>,
) {
    let words = lexed
        .tokens
        .iter()
        .filter_map(|token| match token {
            Token::Word(word) => Some(word),
            Token::OpenParen | Token::CloseParen => None,
        })
        .collect::<Vec<_>>();
    let requirement_words = if words
        .first()
        .is_some_and(|word| !word.quoted && word.value == "require")
    {
        &words[1..]
    } else {
        words.as_slice()
    };
    if contains_parenthesis(&lexed.tokens) || requirement_words.len() != 2 {
        markers.push(malformed_marker(line_start, line_end));
        return;
    }
    let module_path = &requirement_words[0].value;
    let version = &requirement_words[1].value;
    if !is_valid_module_path(module_path) || !is_valid_module_version(module_path, version) {
        markers.push(UnknownMarker {
            reason: UnknownReasonCode::MissingProjectConfig,
            kind: "invalid_require_boundary",
            note: "Go module requirement contains an invalid module path or version boundary",
            start_byte: line_start,
            end_byte: line_end,
        });
        return;
    }
    let direct = !comment_is_indirect(&lexed.comment);
    if candidates.len() <= GO_MOD_MAX_DEPENDENCIES {
        candidates.push(DependencyCandidate {
            module_path: module_path.clone(),
            version: version.clone(),
            direct,
            range: SourceRange::new(requirement_words[0].start, requirement_words[1].end)
                .expect("lexed word ranges are ordered within the source line"),
        });
    }
}

fn comment_is_indirect(comment: &str) -> bool {
    // Mirrors the official x/mod modfile rule: `indirect` must be the only
    // field, or `indirect;` must prefix a non-empty rationale.
    let fields = comment.split_whitespace().collect::<Vec<_>>();
    matches!(fields.as_slice(), ["indirect"])
        || fields.len() > 1 && fields.first() == Some(&"indirect;")
}

fn replace_marker(lexed: &LexedLine, start_byte: usize, end_byte: usize) -> UnknownMarker {
    let words = lexed
        .tokens
        .iter()
        .filter_map(|token| match token {
            Token::Word(word) => Some(word),
            Token::OpenParen | Token::CloseParen => None,
        })
        .collect::<Vec<_>>();
    let body = if words
        .first()
        .is_some_and(|word| !word.quoted && word.value == "replace")
    {
        &words[1..]
    } else {
        words.as_slice()
    };
    let arrows = body
        .iter()
        .enumerate()
        .filter(|(_, word)| !word.quoted && word.value == "=>")
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    let Some(&arrow) = arrows.first().filter(|_| arrows.len() == 1) else {
        return malformed_marker(start_byte, end_byte);
    };
    let original = &body[..arrow];
    let replacement = &body[arrow + 1..];
    let original_valid = match original {
        [path] => is_valid_module_path(&path.value),
        [path, version] => {
            is_valid_module_path(&path.value)
                && is_valid_module_version(&path.value, &version.value)
        }
        _ => false,
    };
    let local = matches!(replacement, [path] if is_local_replace_path(&path.value));
    let replacement_valid = local
        || matches!(replacement, [path, version]
            if is_valid_module_path(&path.value)
                && is_valid_module_version(&path.value, &version.value));
    if !original_valid || !replacement_valid {
        return malformed_marker(start_byte, end_byte);
    }
    UnknownMarker {
        reason: UnknownReasonCode::BuildVariantAmbiguity,
        kind: if local {
            "local_path_replace"
        } else {
            "replace_directive"
        },
        note: if local {
            "Local Go module replacement cannot be represented as a resolved dependency"
        } else {
            "Go module replacement requires unresolved module-graph interpretation"
        },
        start_byte,
        end_byte,
    }
}

fn is_local_replace_path(path: &str) -> bool {
    path == "."
        || path == ".."
        || path.starts_with("./")
        || path.starts_with("../")
        || path.starts_with(".\\")
        || path.starts_with("..\\")
        || path.starts_with('/')
        || path.starts_with('\\')
        || (path.len() >= 2
            && path.as_bytes()[0].is_ascii_alphabetic()
            && path.as_bytes()[1] == b':')
}

fn valid_unsupported_directive_body(directive: BlockDirective, tokens: &[Token]) -> bool {
    let quoted_godebug = directive == BlockDirective::Godebug
        && tokens
            .iter()
            .any(|token| matches!(token, Token::Word(word) if word.quoted));
    let quoted_retract_punctuation = directive == BlockDirective::Retract
        && tokens.iter().any(|token| {
            matches!(token, Token::Word(word)
                if word.quoted
                    && word
                        .value
                        .bytes()
                        .any(|byte| matches!(byte, b'[' | b',' | b']')))
        });
    if quoted_godebug || quoted_retract_punctuation {
        return false;
    }
    let words = tokens
        .iter()
        .map(|token| match token {
            Token::Word(word) => Some(word.value.as_str()),
            Token::OpenParen | Token::CloseParen => None,
        })
        .collect::<Option<Vec<_>>>();
    let Some(words) = words else {
        return false;
    };
    match directive {
        BlockDirective::Exclude => matches!(words.as_slice(), [path, version]
            if is_valid_module_path(path) && is_valid_module_version(path, version)),
        BlockDirective::Retract => {
            let joined = words.concat();
            if let Some(interval) = joined
                .strip_prefix('[')
                .and_then(|value| value.strip_suffix(']'))
            {
                interval.split_once(',').is_some_and(|(low, high)| {
                    !high.contains(',')
                        && parse_semantic_version(low).is_some()
                        && parse_semantic_version(high).is_some()
                })
            } else {
                parse_semantic_version(&joined).is_some()
            }
        }
        BlockDirective::Tool => matches!(words.as_slice(), [value] if !value.is_empty()),
        BlockDirective::Ignore => matches!(words.as_slice(), [value] if !value.is_empty()),
        BlockDirective::Godebug => matches!(words.as_slice(), [setting]
        if setting.split_once('=').is_some_and(|(key, value)| {
            !key.is_empty()
                && !value.is_empty()
                && !value.contains('=')
                && !setting.bytes().any(|byte| matches!(byte, b'"' | b'`' | b'\'' | b','))
        })),
        BlockDirective::Module | BlockDirective::Require | BlockDirective::Replace => false,
    }
}

fn is_valid_toolchain_name(name: &str) -> bool {
    name == "default" || name.strip_prefix("go").is_some_and(is_valid_go_directive)
}

fn unsupported_directive_marker(
    directive: &str,
    start_byte: usize,
    end_byte: usize,
) -> UnknownMarker {
    let (reason, kind, note) = match directive {
        "exclude" => (
            UnknownReasonCode::BuildVariantAmbiguity,
            "exclude_directive",
            "Go module exclusion requires unresolved module-graph interpretation",
        ),
        "retract" => (
            UnknownReasonCode::InsufficientSupport,
            "retract_directive",
            "Go module retraction semantics are outside static dependency inventory",
        ),
        "toolchain" => (
            UnknownReasonCode::BuildVariantAmbiguity,
            "toolchain_directive",
            "Go toolchain selection is unresolved by static dependency inventory",
        ),
        "tool" => (
            UnknownReasonCode::InsufficientSupport,
            "tool_directive",
            "Go tool dependency semantics are outside bounded require inventory",
        ),
        "godebug" => (
            UnknownReasonCode::InsufficientSupport,
            "godebug_directive",
            "Go runtime configuration is outside static dependency inventory",
        ),
        "ignore" => (
            UnknownReasonCode::InsufficientSupport,
            "ignore_directive",
            "Go ignore directive semantics are outside static dependency inventory",
        ),
        "replace" => {
            return UnknownMarker {
                reason: UnknownReasonCode::BuildVariantAmbiguity,
                kind: "replace_directive",
                note: "Go module replacement requires unresolved module-graph interpretation",
                start_byte,
                end_byte,
            }
        }
        _ => (
            UnknownReasonCode::InsufficientSupport,
            "unsupported_or_unknown_directive",
            "Go module dependency inventory contains an unsupported or unknown directive",
        ),
    };
    UnknownMarker {
        reason,
        kind,
        note,
        start_byte,
        end_byte,
    }
}

fn malformed_marker(start_byte: usize, end_byte: usize) -> UnknownMarker {
    UnknownMarker {
        reason: UnknownReasonCode::MissingProjectConfig,
        kind: "malformed_go_mod_syntax",
        note: "Go module dependency inventory contains malformed directive syntax",
        start_byte,
        end_byte,
    }
}

fn dependency_unknown(
    unit: &CodeUnit,
    reason: UnknownReasonCode,
    kind: &str,
    range: SourceRange,
    note: &str,
) -> Result<SemanticFact, ParseError> {
    Ok(SemanticFact {
        kind: SemanticFactKind::Unknown,
        subject: unit.id.as_str().to_string(),
        target: Some(
            SymbolId::new(reason.as_protocol_str().to_string()).map_err(ParseError::Internal)?,
        ),
        origin: FactOrigin {
            engine: GO_CONFIG_ENGINE.to_string(),
            engine_version: env!("CARGO_PKG_VERSION").to_string(),
            method: GO_CONFIG_METHOD.to_string(),
        },
        certainty: FactCertainty::Unknown,
        evidence: Evidence::new(unit.id.clone(), range, unit.provenance.clone(), note)
            .map_err(ParseError::Internal)?,
        assumptions: vec![
            "affected_claim=go_dependency_inventory".to_string(),
            format!("go_unknown_kind={kind}"),
        ],
    })
}

#[derive(Debug)]
struct LexedLine {
    tokens: Vec<Token>,
    comment: String,
}

#[derive(Debug)]
enum Token {
    Word(WordToken),
    OpenParen,
    CloseParen,
}

#[derive(Debug)]
struct WordToken {
    value: String,
    start: usize,
    end: usize,
    quoted: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LexError {
    Malformed,
    ResourceLimit,
}

fn lex_line(line: &str, base_offset: usize) -> Result<LexedLine, LexError> {
    let bytes = line.as_bytes();
    let mut tokens = Vec::new();
    let mut comment = String::new();
    let mut index = 0usize;
    while index < bytes.len() {
        if matches!(bytes[index], b' ' | b'\t' | b'\r') {
            index += 1;
            continue;
        }
        if bytes[index] == b'/' && bytes.get(index + 1) == Some(&b'/') {
            comment = line[index + 2..].trim().to_string();
            break;
        }
        if tokens.len() >= GO_MOD_MAX_TOKENS_PER_LINE {
            return Err(LexError::ResourceLimit);
        }
        match bytes[index] {
            b'(' => {
                tokens.push(Token::OpenParen);
                index += 1;
            }
            b')' => {
                tokens.push(Token::CloseParen);
                index += 1;
            }
            b'"' => {
                let start = index;
                let (value, next) =
                    lex_quoted_word(line, index).map_err(|_| LexError::Malformed)?;
                if next.saturating_sub(start) > GO_MOD_MAX_TOKEN_BYTES {
                    return Err(LexError::ResourceLimit);
                }
                tokens.push(Token::Word(WordToken {
                    value,
                    start: base_offset + start,
                    end: base_offset + next,
                    quoted: true,
                }));
                index = next;
            }
            b'`' => return Err(LexError::Malformed),
            byte if byte.is_ascii_control() => return Err(LexError::Malformed),
            _ => {
                let start = index;
                while index < bytes.len()
                    && !matches!(bytes[index], b' ' | b'\t' | b'\r' | b'(' | b')')
                    && !(bytes[index] == b'/' && bytes.get(index + 1) == Some(&b'/'))
                {
                    if bytes[index].is_ascii_control() {
                        return Err(LexError::Malformed);
                    }
                    index += 1;
                }
                if index == start || index - start > GO_MOD_MAX_TOKEN_BYTES {
                    return if index == start {
                        Err(LexError::Malformed)
                    } else {
                        Err(LexError::ResourceLimit)
                    };
                }
                tokens.push(Token::Word(WordToken {
                    value: line[start..index].to_string(),
                    start: base_offset + start,
                    end: base_offset + index,
                    quoted: false,
                }));
            }
        }
    }
    Ok(LexedLine { tokens, comment })
}

fn lex_quoted_word(line: &str, start: usize) -> Result<(String, usize), ()> {
    let bytes = line.as_bytes();
    let delimiter = bytes[start];
    let mut output = Vec::new();
    let mut index = start + 1;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte == delimiter {
            return String::from_utf8(output)
                .map(|value| (value, index + 1))
                .map_err(|_| ());
        }
        if byte == b'\n' || byte == b'\r' {
            return Err(());
        }
        if byte != b'\\' {
            output.push(byte);
            index += 1;
            continue;
        }
        index += 1;
        let escape = *bytes.get(index).ok_or(())?;
        match escape {
            b'a' => output.push(0x07),
            b'b' => output.push(0x08),
            b'f' => output.push(0x0c),
            b'n' => output.push(b'\n'),
            b'r' => output.push(b'\r'),
            b't' => output.push(b'\t'),
            b'v' => output.push(0x0b),
            b'\\' => output.push(b'\\'),
            b'"' => output.push(b'"'),
            b'\'' => output.push(b'\''),
            b'x' => {
                let value = parse_hex(bytes.get(index + 1..index + 3).ok_or(())?)?;
                output.push(u8::try_from(value).map_err(|_| ())?);
                index += 2;
            }
            b'u' => {
                append_unicode_escape(&mut output, bytes.get(index + 1..index + 5).ok_or(())?)?;
                index += 4;
            }
            b'U' => {
                append_unicode_escape(&mut output, bytes.get(index + 1..index + 9).ok_or(())?)?;
                index += 8;
            }
            b'0'..=b'7' => {
                let digits = bytes.get(index..index + 3).ok_or(())?;
                if !digits.iter().all(|digit| matches!(digit, b'0'..=b'7')) {
                    return Err(());
                }
                let value = digits
                    .iter()
                    .fold(0u16, |value, digit| value * 8 + u16::from(*digit - b'0'));
                output.push(u8::try_from(value).map_err(|_| ())?);
                index += 2;
            }
            _ => return Err(()),
        }
        index += 1;
    }
    Err(())
}

fn append_unicode_escape(output: &mut Vec<u8>, digits: &[u8]) -> Result<(), ()> {
    let value = parse_hex(digits)?;
    let character = char::from_u32(value).ok_or(())?;
    let mut encoded = [0u8; 4];
    output.extend_from_slice(character.encode_utf8(&mut encoded).as_bytes());
    Ok(())
}

fn parse_hex(digits: &[u8]) -> Result<u32, ()> {
    digits.iter().try_fold(0u32, |value, digit| {
        char::from(*digit)
            .to_digit(16)
            .map(|digit| value.saturating_mul(16).saturating_add(digit))
            .ok_or(())
    })
}

fn word_at(tokens: &[Token], index: usize) -> Option<&str> {
    match tokens.get(index) {
        Some(Token::Word(word)) => Some(&word.value),
        Some(Token::OpenParen | Token::CloseParen) | None => None,
    }
}

fn unquoted_word_at(tokens: &[Token], index: usize) -> Option<&str> {
    match tokens.get(index) {
        Some(Token::Word(word)) if !word.quoted => Some(&word.value),
        Some(Token::Word(_) | Token::OpenParen | Token::CloseParen) | None => None,
    }
}

fn is_close_line(tokens: &[Token]) -> bool {
    matches!(tokens, [Token::CloseParen])
}

fn contains_parenthesis(tokens: &[Token]) -> bool {
    tokens
        .iter()
        .any(|token| matches!(token, Token::OpenParen | Token::CloseParen))
}

fn is_valid_module_path(path: &str) -> bool {
    if path.is_empty() || !path.is_ascii() {
        return false;
    }
    let first = path.split('/').next().unwrap_or(path);
    if first.starts_with('-')
        || !first.contains('.')
        || !first.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'.')
        })
    {
        return false;
    }
    if path.split('/').any(|component| {
        let short = component.split('.').next().unwrap_or(component);
        short.rsplit_once('~').is_some_and(|(_, suffix)| {
            !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit())
        })
    }) {
        return false;
    }
    let last = path.rsplit('/').next().unwrap_or(path);
    if path.starts_with("gopkg.in/") {
        let versioned = last.strip_suffix("-unstable").unwrap_or(last);
        let Some((prefix, suffix)) = versioned.rsplit_once(".v") else {
            return false;
        };
        if prefix.is_empty()
            || suffix.is_empty()
            || !suffix.bytes().all(|byte| byte.is_ascii_digit())
            || suffix.len() > 1 && suffix.starts_with('0')
        {
            return false;
        }
    } else if let Some(suffix) = last.strip_prefix('v') {
        if !suffix.is_empty()
            && suffix
                .bytes()
                .all(|byte| byte.is_ascii_digit() || byte == b'.')
            && (suffix.bytes().any(|byte| byte == b'.') || suffix.starts_with('0') || suffix == "1")
        {
            return false;
        }
    }
    path.split('/').all(|component| {
        !component.is_empty()
            && !component.starts_with('.')
            && !component.ends_with('.')
            && !component.contains("..")
            && !is_windows_reserved_name(component)
            && component.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~')
            })
    })
}

fn is_windows_reserved_name(component: &str) -> bool {
    let stem = component
        .split('.')
        .next()
        .unwrap_or(component)
        .to_ascii_lowercase();
    matches!(stem.as_str(), "con" | "prn" | "aux" | "nul")
        || stem
            .strip_prefix("com")
            .or_else(|| stem.strip_prefix("lpt"))
            .is_some_and(|suffix| {
                matches!(suffix, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")
            })
}

fn is_valid_go_directive(version: &str) -> bool {
    let suffix_start = version
        .bytes()
        .position(|byte| byte.is_ascii_lowercase())
        .unwrap_or(version.len());
    let (numeric, suffix) = version.split_at(suffix_start);
    if !suffix.is_empty() {
        let digit_start = suffix
            .bytes()
            .position(|byte| byte.is_ascii_digit())
            .unwrap_or(suffix.len());
        let (label, digits) = suffix.split_at(digit_start);
        if label.is_empty()
            || digits.is_empty()
            || !label.bytes().all(|byte| byte.is_ascii_lowercase())
            || !digits.bytes().all(|byte| byte.is_ascii_digit())
        {
            return false;
        }
    }
    let components = numeric.split('.').collect::<Vec<_>>();
    matches!(components.len(), 2 | 3)
        && components.iter().enumerate().all(|(index, value)| {
            !value.is_empty()
                && value.bytes().all(|byte| byte.is_ascii_digit())
                && (*value == "0" || !value.starts_with('0'))
                && (index != 0 || *value != "0")
        })
}

fn is_valid_module_version(path: &str, version: &str) -> bool {
    let Some((major, has_incompatible)) = parse_semantic_version(version) else {
        return false;
    };
    match module_path_major(path) {
        ModulePathMajor::Exact(path_major) => path_major == major,
        ModulePathMajor::GopkgV1 => major == "1" || version.starts_with("v0.0.0-"),
        ModulePathMajor::Unsuffixed if matches!(major, "0" | "1") => true,
        ModulePathMajor::Unsuffixed => has_incompatible,
    }
}

fn parse_semantic_version(version: &str) -> Option<(&str, bool)> {
    if version.is_empty() || !version.is_ascii() {
        return None;
    }
    let version = version.strip_prefix('v')?;
    let (without_build, build) = version
        .split_once('+')
        .map_or((version, None), |(left, right)| (left, Some(right)));
    if build.is_some_and(|value| !valid_semver_identifiers(value, false) || value != "incompatible")
        || without_build.contains('+')
    {
        return None;
    }
    let (core, prerelease) = without_build
        .split_once('-')
        .map_or((without_build, None), |(left, right)| (left, Some(right)));
    if prerelease.is_some_and(|value| !valid_semver_identifiers(value, true)) {
        return None;
    }
    let components = core.split('.').collect::<Vec<_>>();
    if components.len() != 3 || !components.iter().all(|value| valid_semver_number(value)) {
        return None;
    }
    Some((components[0], build == Some("incompatible")))
}

fn valid_semver_number(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|byte| byte.is_ascii_digit())
        && (value == "0" || !value.starts_with('0'))
}

fn valid_semver_identifiers(value: &str, reject_numeric_leading_zero: bool) -> bool {
    !value.is_empty()
        && value.split('.').all(|identifier| {
            !identifier.is_empty()
                && identifier
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
                && (!reject_numeric_leading_zero
                    || !identifier.bytes().all(|byte| byte.is_ascii_digit())
                    || valid_semver_number(identifier))
        })
}

enum ModulePathMajor<'a> {
    Unsuffixed,
    Exact(&'a str),
    GopkgV1,
}

fn module_path_major(path: &str) -> ModulePathMajor<'_> {
    let last = path.rsplit('/').next().unwrap_or(path);
    let suffix = if path.starts_with("gopkg.in/") {
        let versioned = last.strip_suffix("-unstable").unwrap_or(last);
        let Some((_, suffix)) = versioned.rsplit_once(".v") else {
            return ModulePathMajor::Unsuffixed;
        };
        if suffix == "1" {
            return ModulePathMajor::GopkgV1;
        }
        suffix
    } else if let Some(suffix) = last.strip_prefix('v') {
        suffix
    } else {
        return ModulePathMajor::Unsuffixed;
    };
    if path.starts_with("gopkg.in/")
        || (!suffix.is_empty()
            && suffix.bytes().all(|byte| byte.is_ascii_digit())
            && !suffix.starts_with('0')
            && suffix != "1")
    {
        ModulePathMajor::Exact(suffix)
    } else {
        ModulePathMajor::Unsuffixed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::{ContentHash, RepositoryRevision};

    fn parse_config(text: &str, path: &str) -> SourceParseOutput {
        parse_output(SourceDocument {
            path,
            language: Language::GoConfig,
            content_hash: ContentHash::new(
                "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            )
            .expect("valid content hash"),
            repository_revision: RepositoryRevision::new("UNKNOWN").expect("valid revision"),
            text,
        })
        .expect("parse Go project config")
    }

    fn unknown_kinds(output: &SourceParseOutput) -> BTreeSet<&str> {
        output
            .report
            .semantic_facts
            .iter()
            .flat_map(|fact| &fact.assumptions)
            .filter_map(|assumption| assumption.strip_prefix("go_unknown_kind="))
            .collect()
    }

    fn manifest_padded_to(byte_count: usize) -> String {
        let mut text = "module example.test/app\n".to_string();
        while byte_count.saturating_sub(text.len()) > GO_MOD_MAX_LINE_BYTES {
            text.push_str("//");
            text.push_str(&"x".repeat(GO_MOD_MAX_LINE_BYTES - 3));
            text.push('\n');
        }
        let remaining = byte_count.saturating_sub(text.len());
        if remaining == 1 {
            text.push(' ');
        } else if remaining > 1 {
            text.push_str("//");
            text.push_str(&"x".repeat(remaining - 2));
        }
        assert_eq!(text.len(), byte_count);
        text
    }

    #[test]
    fn inventories_direct_indirect_nested_and_quoted_requirements() {
        let source = "module example.test/app\n\
             go 1.25.0\n\
             require example.test/direct v1.2.3\n\
             require (\n\
                 example.test/indirect v0.0.0-20250701000000-abcdefabcdef // indirect\n\
                 example.test/indirect-note v1.0.0 // indirect; retained rationale\n\
                 \"example.test/quoted\" \"v1.0.0\"\n\
                 \"example\\x2etest/escaped\" \"v1.0.0\"\n\
             )\n";
        let output = parse_config(source, "nested/go.mod");
        assert_eq!(output.report.units.len(), 1);
        assert_eq!(output.dependencies.len(), 5);
        assert_eq!(
            output
                .dependencies
                .iter()
                .map(|dependency| (
                    dependency.package.name.as_str(),
                    dependency
                        .requirement
                        .as_ref()
                        .map(DependencyVersion::as_str),
                    dependency.directness,
                    dependency.scope,
                    dependency.evidence_level,
                ))
                .collect::<Vec<_>>(),
            vec![
                (
                    "example.test/direct",
                    Some("v1.2.3"),
                    DependencyDirectness::Direct,
                    DependencyScope::Unknown,
                    DependencyEvidenceLevel::ManifestDeclared,
                ),
                (
                    "example.test/escaped",
                    Some("v1.0.0"),
                    DependencyDirectness::Direct,
                    DependencyScope::Unknown,
                    DependencyEvidenceLevel::ManifestDeclared,
                ),
                (
                    "example.test/indirect",
                    Some("v0.0.0-20250701000000-abcdefabcdef"),
                    DependencyDirectness::Transitive,
                    DependencyScope::Unknown,
                    DependencyEvidenceLevel::ManifestDeclared,
                ),
                (
                    "example.test/indirect-note",
                    Some("v1.0.0"),
                    DependencyDirectness::Transitive,
                    DependencyScope::Unknown,
                    DependencyEvidenceLevel::ManifestDeclared,
                ),
                (
                    "example.test/quoted",
                    Some("v1.0.0"),
                    DependencyDirectness::Direct,
                    DependencyScope::Unknown,
                    DependencyEvidenceLevel::ManifestDeclared,
                ),
            ]
        );
        assert!(output.report.semantic_facts.is_empty());
        assert!(output.dependencies.iter().all(|dependency| {
            dependency.package.ecosystem == DependencyEcosystem::GoModules
                && dependency.resolved_version.is_none()
                && !dependency.optional
                && dependency.evidence.provenance.path == "nested/go.mod"
                && dependency.evidence.note == "bounded go.mod require declaration"
        }));
        let direct = output
            .dependencies
            .iter()
            .find(|dependency| dependency.package.name == "example.test/direct")
            .expect("direct dependency");
        assert_eq!(
            &source[direct.evidence.range.start_byte..direct.evidence.range.end_byte],
            "example.test/direct v1.2.3"
        );
    }

    #[test]
    fn rejects_invalid_path_version_and_major_boundaries() {
        let output = parse_config(
            "module example.test/app\n\
             require example.test/ok/v2 v2.1.0\n\
             require example.test/no-major v2.1.0\n\
             require example.test/bad@path v1.0.0\n\
             require example.test/bad-version 1.0.0\n",
            "go.mod",
        );
        assert!(output.dependencies.is_empty());
        assert!(unknown_kinds(&output).contains("invalid_require_boundary"));
    }

    #[test]
    fn conflicting_and_graph_changing_inputs_omit_only_ambiguous_requirements() {
        let output = parse_config(
            "module example.test/app\n\
             require example.test/conflict v1.0.0\n\
             require example.test/conflict v1.1.0 // indirect\n\
             require example.test/kept v1.0.0\n\
             replace example.test/kept => ../local\n\
             exclude example.test/kept v1.0.0\n\
             retract \"v1.0.0\"\n\
             retract [ \"v1.0.0\" , \"v1.1.0\" ]\n\
             toolchain go1.25.1\n\
             tool \"example.test/tool\"\n",
            "go.mod",
        );
        assert_eq!(output.dependencies.len(), 1);
        assert_eq!(output.dependencies[0].package.name, "example.test/kept");
        let kinds = unknown_kinds(&output);
        for expected in [
            "duplicate_or_conflicting_require",
            "local_path_replace",
            "exclude_directive",
            "retract_directive",
            "toolchain_directive",
            "tool_directive",
        ] {
            assert!(kinds.contains(expected), "missing {expected}: {kinds:?}");
        }
        assert!(output.report.semantic_facts.iter().all(|fact| {
            fact.kind == SemanticFactKind::Unknown
                && fact.certainty == FactCertainty::Unknown
                && fact
                    .assumptions
                    .contains(&"affected_claim=go_dependency_inventory".to_string())
                && !fact.evidence.note.contains("example.test")
        }));
    }

    #[test]
    fn malformed_manifest_fails_closed_without_partial_dependency_records() {
        for text in [
            "module example.test/app\nrequire example.test/kept v1.0.0\nrequire missing-version\n",
            "module example.test/app\nrequire (\nexample.test/kept v1.0.0\n",
            "require example.test/kept v1.0.0\n",
            "module example.test/app\nmodule example.test/other\nrequire example.test/kept v1.0.0\n",
            "module example.test/app\ngo 1.25\ngo 1.26\nrequire example.test/kept v1.0.0\n",
            "module example.test/app\nrequire example.test/kept v1.0.0\nreplace example.test/kept => invalid-module-path\n",
            "module example.test/app\nrequire example.test/kept v1.0.0\nexclude example.test/kept\n",
            "module example.test/app\nrequire example.test/kept v1.0.0\ntoolchain not-a-toolchain\n",
            "module example.test/app\nrequire example.test/kept v1.0.0\nrequire `example.test/raw` `v1.0.0`\n",
        ] {
            let output = parse_config(text, "go.mod");
            assert!(output.dependencies.is_empty(), "{text:?}");
            assert!(output.report.semantic_facts.iter().any(|fact| {
                fact.kind == SemanticFactKind::Unknown
                    && fact.certainty == FactCertainty::Unknown
                    && fact
                        .assumptions
                        .contains(&"affected_claim=go_dependency_inventory".to_string())
            }));
        }
    }

    #[test]
    fn quoted_directive_positions_are_malformed_and_fail_closed() {
        for invalid_line in [
            "\"require\" example.test/other v1.0.0",
            "\"module\" example.test/other",
            "go \"1.25\"",
            "toolchain \"go1.25\"",
            "godebug \"setting=1\"",
            "replace example.test/kept \"=>\" ../local",
            "retract \"[v1.0.0,v1.1.0]\"",
        ] {
            let output = parse_config(
                &format!(
                    "module example.test/app\nrequire example.test/kept v1.0.0\n{invalid_line}\n"
                ),
                "go.mod",
            );
            assert!(output.dependencies.is_empty(), "{invalid_line}");
            assert!(
                unknown_kinds(&output).contains("malformed_go_mod_syntax"),
                "{invalid_line}"
            );
        }
    }

    #[test]
    fn resource_limits_are_inclusive_and_fail_closed_above_the_boundary() {
        let manifest = |dependency_count: usize| {
            let mut text = "module example.test/app\nrequire (\n".to_string();
            for index in 0..dependency_count {
                text.push_str(&format!("example.test/dependency-{index} v1.0.0\n"));
            }
            text.push_str(")\n");
            text
        };
        let maximum = parse_config(&manifest(GO_MOD_MAX_DEPENDENCIES), "go.mod");
        assert_eq!(maximum.dependencies.len(), GO_MOD_MAX_DEPENDENCIES);
        assert!(!unknown_kinds(&maximum).contains("dependency_limit"));

        let excessive = parse_config(&manifest(GO_MOD_MAX_DEPENDENCIES + 1), "go.mod");
        assert!(excessive.dependencies.is_empty());
        assert!(unknown_kinds(&excessive).contains("dependency_limit"));

        let prefix = "module example.test/app //";
        let maximum_line = format!(
            "{prefix}{}",
            "x".repeat(GO_MOD_MAX_LINE_BYTES - prefix.len())
        );
        let maximum_line_output = parse_config(&maximum_line, "go.mod");
        assert!(maximum_line_output.report.semantic_facts.is_empty());
        let excessive_line = format!("{maximum_line}x");
        let excessive_line_output = parse_config(&excessive_line, "go.mod");
        assert!(excessive_line_output.dependencies.is_empty());
        assert!(unknown_kinds(&excessive_line_output).contains("manifest_resource_limit"));

        let excessive_bytes = " ".repeat(GO_MOD_MAX_BYTES + 1);
        let excessive_bytes_output = parse_config(&excessive_bytes, "go.mod");
        assert!(excessive_bytes_output.dependencies.is_empty());
        assert!(unknown_kinds(&excessive_bytes_output).contains("manifest_byte_limit"));

        let maximum_bytes = manifest_padded_to(GO_MOD_MAX_BYTES);
        let maximum_bytes_output = parse_config(&maximum_bytes, "go.mod");
        assert!(!unknown_kinds(&maximum_bytes_output).contains("manifest_byte_limit"));

        let maximum_lines = format!(
            "module example.test/app\n{}",
            "\n".repeat(GO_MOD_MAX_LINES - 1)
        );
        let maximum_lines_output = parse_config(&maximum_lines, "go.mod");
        assert!(!unknown_kinds(&maximum_lines_output).contains("manifest_resource_limit"));
        let excessive_lines_output = parse_config(&format!("{maximum_lines}\n"), "go.mod");
        assert!(unknown_kinds(&excessive_lines_output).contains("manifest_resource_limit"));

        let mut maximum_directives = "module example.test/app\n".to_string();
        for _ in 1..GO_MOD_MAX_DIRECTIVES {
            maximum_directives.push_str("godebug setting=1\n");
        }
        let maximum_directives_output = parse_config(&maximum_directives, "go.mod");
        assert!(!unknown_kinds(&maximum_directives_output).contains("directive_limit"));
        maximum_directives.push_str("godebug setting=1\n");
        let excessive_directives_output = parse_config(&maximum_directives, "go.mod");
        assert!(unknown_kinds(&excessive_directives_output).contains("directive_limit"));

        let exact_tokens = "word ".repeat(GO_MOD_MAX_TOKENS_PER_LINE);
        assert!(lex_line(&exact_tokens, 0).is_ok());
        let exact_token_manifest =
            format!("module example.test/app\nrequire example.test/kept v1.0.0\n{exact_tokens}\n");
        let exact_token_output = parse_config(&exact_token_manifest, "go.mod");
        assert_eq!(exact_token_output.dependencies.len(), 1);
        assert!(!unknown_kinds(&exact_token_output).contains("manifest_token_limit"));
        let excessive_tokens = "word ".repeat(GO_MOD_MAX_TOKENS_PER_LINE + 1);
        assert!(matches!(
            lex_line(&excessive_tokens, 0),
            Err(LexError::ResourceLimit)
        ));
        let excessive_token_output = parse_config(
            &format!(
                "module example.test/app\nrequire example.test/kept v1.0.0\n{excessive_tokens}\n"
            ),
            "go.mod",
        );
        assert!(excessive_token_output.dependencies.is_empty());
        assert!(unknown_kinds(&excessive_token_output).contains("manifest_token_limit"));

        let exact_token_bytes = "x".repeat(GO_MOD_MAX_TOKEN_BYTES);
        assert!(lex_line(&exact_token_bytes, 0).is_ok());
        let exact_token_bytes_output = parse_config(
            &format!(
                "module example.test/app\nrequire example.test/kept v1.0.0\n{exact_token_bytes}\n"
            ),
            "go.mod",
        );
        assert_eq!(exact_token_bytes_output.dependencies.len(), 1);
        assert!(!unknown_kinds(&exact_token_bytes_output).contains("manifest_token_limit"));
        let excessive_token_bytes = "x".repeat(GO_MOD_MAX_TOKEN_BYTES + 1);
        assert!(matches!(
            lex_line(&excessive_token_bytes, 0),
            Err(LexError::ResourceLimit)
        ));
        let excessive_token_bytes_output = parse_config(
            &format!(
                "module example.test/app\nrequire example.test/kept v1.0.0\n{excessive_token_bytes}\n"
            ),
            "go.mod",
        );
        assert!(excessive_token_bytes_output.dependencies.is_empty());
        assert!(unknown_kinds(&excessive_token_bytes_output).contains("manifest_token_limit"));
    }

    #[test]
    fn go_workspace_is_typed_unknown_without_dependency_claims() {
        let output = parse_config("go 1.25.0\nuse ./module\n", "go.work");
        assert!(output.dependencies.is_empty());
        assert_eq!(
            unknown_kinds(&output),
            BTreeSet::from(["workspace_configuration"])
        );
        assert_eq!(
            output.report.semantic_facts[0]
                .target
                .as_ref()
                .map(SymbolId::as_str),
            Some("BuildVariantAmbiguity")
        );

        let exact_bytes = parse_config(&manifest_padded_to(GO_MOD_MAX_BYTES), "go.work");
        assert_eq!(
            unknown_kinds(&exact_bytes),
            BTreeSet::from(["workspace_configuration"])
        );
        let excessive_bytes = parse_config(&"x".repeat(GO_MOD_MAX_BYTES + 1), "go.work");
        assert_eq!(
            unknown_kinds(&excessive_bytes),
            BTreeSet::from(["workspace_byte_limit"])
        );

        let exact_lines = "\n".repeat(GO_MOD_MAX_LINES);
        let exact_lines_output = parse_config(&exact_lines, "go.work");
        assert_eq!(
            unknown_kinds(&exact_lines_output),
            BTreeSet::from(["workspace_configuration"])
        );
        let excessive_lines = parse_config(&format!("{exact_lines}\n"), "go.work");
        assert_eq!(
            unknown_kinds(&excessive_lines),
            BTreeSet::from(["workspace_resource_limit"])
        );

        let exact_line = parse_config(&"x".repeat(GO_MOD_MAX_LINE_BYTES), "go.work");
        assert_eq!(
            unknown_kinds(&exact_line),
            BTreeSet::from(["workspace_configuration"])
        );
        let excessive_line = parse_config(&"x".repeat(GO_MOD_MAX_LINE_BYTES + 1), "go.work");
        assert_eq!(
            unknown_kinds(&excessive_line),
            BTreeSet::from(["workspace_resource_limit"])
        );
    }

    #[test]
    fn semantic_version_and_module_path_validation_cover_go_boundaries() {
        let module_block = parse_config(
            "module (\nexample.test/app\n)\nrequire example.test/lib v1.0.0\n",
            "go.mod",
        );
        assert_eq!(module_block.dependencies.len(), 1);
        assert!(module_block.report.semantic_facts.is_empty());

        for version in ["1.25", "1.25.0", "1.25rc1", "2.0"] {
            assert!(is_valid_go_directive(version), "{version}");
        }
        for version in ["1", "01.25", "1.025", "1.25RC1", "1.25rc", "v1.25"] {
            assert!(!is_valid_go_directive(version), "{version}");
        }
        for name in ["default", "go1.25", "go1.25.1", "go1.25rc1"] {
            assert!(is_valid_toolchain_name(name), "{name}");
        }
        for name in ["go1", "go1.", "go1.foo", "go01.25", "local"] {
            assert!(!is_valid_toolchain_name(name), "{name}");
        }
        for (path, version) in [
            ("example.test/mod", "v0.0.0-20250101000000-abcdefabcdef"),
            ("example.test/mod", "v2.0.0+incompatible"),
            ("example.test/mod/v2", "v2.0.0+incompatible"),
            ("example.test/mod/v3", "v3.2.1-pre.1"),
            ("gopkg.in/yaml.v3", "v3.0.1"),
            ("gopkg.in/yaml.v3", "v3.0.1+incompatible"),
            ("gopkg.in/check.v1", "v0.0.0-20161208181325-20d25e280405"),
        ] {
            assert!(is_valid_module_version(path, version), "{path} {version}");
        }
        for (path, version) in [
            ("example.test/mod", "v2.0.0"),
            ("example.test/mod/v3", "v2.0.0"),
            ("gopkg.in/check.v1", "v2.0.0+incompatible"),
            ("gopkg.in/check.v1", "v0.2.0"),
            ("example.test/mod", "v01.2.3"),
            ("example.test/mod", "v1.2"),
            ("example.test/mod", "v1.2.3-01"),
            ("example.test/mod", "v1.2.3+meta"),
        ] {
            assert!(!is_valid_module_version(path, version), "{path} {version}");
        }
        for path in [
            "example.test/mod",
            "example.test/module_name",
            "example.test/~module",
            "example.test/Module",
            "gopkg.in/yaml.v3-unstable",
        ] {
            assert!(is_valid_module_path(path), "{path}");
        }
        for path in [
            "",
            "example",
            "-example.test/module",
            "Example.test/module",
            "/module",
            "example.test//mod",
            "example..test/module",
            "example.test/.mod",
            "example.test/bad..path",
            "example.test/CON",
            "example.test/foo~1.txt",
            "example.test/module/v2.3",
            "gopkg.in/yaml",
            "gopkg.in/yaml.v03",
        ] {
            assert!(!is_valid_module_path(path), "{path:?}");
        }
    }
}
