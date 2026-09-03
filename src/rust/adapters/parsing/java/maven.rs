//! Bounded, non-executing Maven POM dependency inventory.
//!
//! This parser accepts only supplied bytes from an exact `pom.xml`. It does not
//! construct Maven's effective model, resolve parents or properties, activate
//! profiles, inspect repositories, execute plugins, invoke Maven/Gradle/javac,
//! or read dependency artifacts. Only direct declarations at
//! `project/dependencies/dependency` become dependency records. Every wider
//! Maven model obligation remains a claim-scoped typed `UNKNOWN`.

use super::super::{config_source_parse_output, sort_inventory_facts};
use crate::core::model::{
    CodeUnit, DependencyDirectness, DependencyEcosystem, DependencyEvidenceLevel, DependencyRecord,
    DependencyScope, DependencyVersion, Evidence, FactCertainty, FactOrigin, Language,
    PackageIdentity, SemanticFact, SemanticFactKind, SourceRange, SymbolId, UnknownReasonCode,
};
use crate::ports::file_discovery::DEFAULT_MAX_FILE_BYTES;
use crate::ports::parser::{
    ParseError, ParseReport, ParserProjectContext, SourceDocument, SourceParseOutput, SourceParser,
};
use std::collections::{BTreeMap, BTreeSet};

const MAVEN_CONFIG_ENGINE: &str = "repogrammar-java-project-config";
const MAVEN_CONFIG_METHOD: &str = "bounded_maven_pom_dependency_inventory_v1";
const MAVEN_DEPENDENCY_LIMIT: usize = 2_000;
const MAVEN_XML_TOKEN_LIMIT: usize = 50_000;
const MAVEN_XML_DEPTH_LIMIT: usize = 128;
const MAVEN_XML_NAME_LIMIT: usize = 256;
const MAVEN_FIELD_TEXT_LIMIT: usize = 1_024;
const MAVEN_COORDINATE_PART_LIMIT: usize = 255;
const MAVEN_VERSION_LIMIT: usize = 256;

#[derive(Debug, Default)]
pub struct JavaMavenConfigParser;

impl SourceParser for JavaMavenConfigParser {
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
    if document.language != Language::JavaConfig
        || document.path.rsplit('/').next().unwrap_or(document.path) != "pom.xml"
    {
        return Err(ParseError::UnsupportedLanguage);
    }

    let unit = super::super::project_config_unit(&document, Language::JavaConfig)?;

    let (mut facts, dependencies) = pom_inventory(document.text, &unit)?;
    sort_inventory_facts(&mut facts);
    config_source_parse_output(vec![unit], facts, dependencies)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PomFailure {
    Malformed,
    UnsupportedXml,
    ResourceLimit,
}

#[derive(Debug, Default)]
struct PomDependencyCandidate {
    start_byte: usize,
    end_byte: usize,
    fields: BTreeMap<String, String>,
    duplicate_fields: BTreeSet<String>,
    unsupported_nested_shape: bool,
}

impl PomDependencyCandidate {
    fn insert_field(&mut self, name: &str, value: String) {
        if self.fields.insert(name.to_string(), value).is_some() {
            self.duplicate_fields.insert(name.to_string());
        }
    }
}

#[derive(Debug)]
struct ActiveField {
    name: String,
    depth: usize,
    text: String,
    invalid: bool,
}

#[derive(Debug)]
struct UnknownMarker {
    reason: &'static str,
    note: &'static str,
}

fn pom_inventory(
    text: &str,
    unit: &CodeUnit,
) -> Result<(Vec<SemanticFact>, Vec<DependencyRecord>), ParseError> {
    let candidates = match parse_pom_candidates(text) {
        Ok(candidates) => candidates,
        Err(PomFailure::Malformed) => {
            return failed_inventory(
                unit,
                UnknownReasonCode::MissingProjectConfig.as_protocol_str(),
                "malformed_maven_pom",
                "Maven dependency inventory is unavailable because pom.xml is malformed",
            );
        }
        Err(PomFailure::UnsupportedXml) => {
            return failed_inventory(
                unit,
                UnknownReasonCode::InsufficientSupport.as_protocol_str(),
                "unsupported_maven_xml_construct",
                "Maven dependency inventory abstained on unsupported XML markup or namespaced elements",
            );
        }
        Err(PomFailure::ResourceLimit) => {
            return failed_inventory(
                unit,
                "ResourceLimit",
                "maven_pom_resource_limit",
                "Maven dependency inventory exceeded a bounded XML or dependency limit",
            );
        }
    };

    let mut markers = candidates.markers;
    let mut records_by_name = BTreeMap::<String, DependencyRecord>::new();
    let mut conflicts = BTreeSet::new();
    for candidate in candidates.dependencies {
        let Some((name, record)) = candidate_to_record(candidate, unit, &mut markers)? else {
            continue;
        };
        if conflicts.contains(&name) {
            continue;
        }
        if records_by_name.insert(name.clone(), record).is_some() {
            records_by_name.remove(&name);
            conflicts.insert(name);
        }
    }
    if !conflicts.is_empty() {
        add_marker(
            &mut markers,
            "conflicting_maven_dependency_declarations",
            UnknownReasonCode::ConflictingFacts.as_protocol_str(),
            "Duplicate Maven dependency identities were omitted because the effective declaration is ambiguous",
        );
    }

    let facts = markers
        .into_iter()
        .map(|(kind, marker)| maven_unknown(unit, marker.reason, &kind, marker.note))
        .collect::<Result<Vec<_>, _>>()?;
    Ok((facts, records_by_name.into_values().collect()))
}

#[derive(Debug, Default)]
struct PomCandidates {
    dependencies: Vec<PomDependencyCandidate>,
    markers: BTreeMap<String, UnknownMarker>,
}

fn parse_pom_candidates(text: &str) -> Result<PomCandidates, PomFailure> {
    if u64::try_from(text.len()).unwrap_or(u64::MAX) > DEFAULT_MAX_FILE_BYTES {
        return Err(PomFailure::ResourceLimit);
    }
    let events = xml_events(text)?;
    let mut stack = Vec::<String>::new();
    let mut output = PomCandidates::default();
    let mut current = None::<PomDependencyCandidate>;
    let mut active_field = None::<ActiveField>;
    let mut root_seen = false;
    let mut root_closed = false;

    for event in events {
        match event {
            XmlEvent::Start {
                name,
                start_byte,
                end_byte,
                self_closing,
            } => {
                if root_closed {
                    return Err(PomFailure::Malformed);
                }
                if stack.is_empty() {
                    if root_seen || name != "project" {
                        return Err(PomFailure::Malformed);
                    }
                    root_seen = true;
                }

                if let Some(field) = active_field.as_mut() {
                    field.invalid = true;
                }
                register_model_obligation(&stack, &name, &mut output.markers);

                if stack_is(&stack, &["project", "dependencies"]) && name == "dependency" {
                    if current.is_some() {
                        return Err(PomFailure::Malformed);
                    }
                    current = Some(PomDependencyCandidate {
                        start_byte,
                        ..PomDependencyCandidate::default()
                    });
                    if output.dependencies.len() >= MAVEN_DEPENDENCY_LIMIT {
                        return Err(PomFailure::ResourceLimit);
                    }
                } else if stack_is(&stack, &["project", "dependencies", "dependency"])
                    && current.is_some()
                {
                    if matches!(
                        name.as_str(),
                        "groupId"
                            | "artifactId"
                            | "version"
                            | "scope"
                            | "optional"
                            | "type"
                            | "classifier"
                    ) {
                        active_field = Some(ActiveField {
                            name: name.clone(),
                            depth: stack.len() + 1,
                            text: String::new(),
                            invalid: false,
                        });
                    } else {
                        current
                            .as_mut()
                            .expect("checked current dependency")
                            .unsupported_nested_shape = true;
                    }
                }

                stack.push(name.clone());
                if stack.len() > MAVEN_XML_DEPTH_LIMIT {
                    return Err(PomFailure::ResourceLimit);
                }
                if self_closing {
                    close_element(
                        &mut stack,
                        &name,
                        end_byte,
                        &mut current,
                        &mut active_field,
                        &mut output,
                        &mut root_closed,
                    )?;
                }
            }
            XmlEvent::End { name, end_byte } => {
                close_element(
                    &mut stack,
                    &name,
                    end_byte,
                    &mut current,
                    &mut active_field,
                    &mut output,
                    &mut root_closed,
                )?;
            }
            XmlEvent::Text { text } => {
                if stack.is_empty() {
                    if !text.trim().is_empty() {
                        return Err(PomFailure::Malformed);
                    }
                    continue;
                }
                if let Some(field) = active_field.as_mut() {
                    if stack.len() == field.depth {
                        if field.text.len().saturating_add(text.len()) > MAVEN_FIELD_TEXT_LIMIT {
                            return Err(PomFailure::ResourceLimit);
                        }
                        field.text.push_str(&text);
                    }
                }
            }
        }
    }

    if !root_seen
        || !root_closed
        || !stack.is_empty()
        || current.is_some()
        || active_field.is_some()
    {
        return Err(PomFailure::Malformed);
    }
    Ok(output)
}

#[allow(clippy::too_many_arguments)]
fn close_element(
    stack: &mut Vec<String>,
    name: &str,
    end_byte: usize,
    current: &mut Option<PomDependencyCandidate>,
    active_field: &mut Option<ActiveField>,
    output: &mut PomCandidates,
    root_closed: &mut bool,
) -> Result<(), PomFailure> {
    if stack.last().map(String::as_str) != Some(name) {
        return Err(PomFailure::Malformed);
    }

    if active_field
        .as_ref()
        .is_some_and(|field| field.depth == stack.len() && field.name == name)
    {
        let field = active_field.take().expect("active field was checked");
        let candidate = current.as_mut().ok_or(PomFailure::Malformed)?;
        if field.invalid {
            candidate.unsupported_nested_shape = true;
        } else {
            candidate.insert_field(&field.name, field.text.trim().to_string());
        }
    }

    if stack_is(stack, &["project", "dependencies", "dependency"]) && name == "dependency" {
        let mut candidate = current.take().ok_or(PomFailure::Malformed)?;
        candidate.end_byte = end_byte;
        output.dependencies.push(candidate);
    }

    stack.pop();
    if name == "project" && stack.is_empty() {
        *root_closed = true;
    }
    Ok(())
}

fn register_model_obligation(
    stack: &[String],
    name: &str,
    markers: &mut BTreeMap<String, UnknownMarker>,
) {
    if stack_is(stack, &["project"]) {
        match name {
            "parent" => add_marker(
                markers,
                "unresolved_maven_parent_model",
                UnknownReasonCode::MissingProjectConfig.as_protocol_str(),
                "Parent inheritance is not resolved by the bounded static POM reader",
            ),
            "dependencyManagement" => add_marker(
                markers,
                "unresolved_maven_dependency_management",
                UnknownReasonCode::InsufficientSupport.as_protocol_str(),
                "dependencyManagement and imported BOM semantics require an effective Maven model",
            ),
            "profiles" => add_marker(
                markers,
                "unresolved_maven_profile_selection",
                UnknownReasonCode::BuildVariantAmbiguity.as_protocol_str(),
                "Maven profile activation and profile-scoped dependencies are not selected",
            ),
            "properties" => add_marker(
                markers,
                "unresolved_maven_properties",
                UnknownReasonCode::MissingProjectConfig.as_protocol_str(),
                "Maven property interpolation is not evaluated",
            ),
            "modules" => add_marker(
                markers,
                "unresolved_maven_reactor_modules",
                UnknownReasonCode::BuildVariantAmbiguity.as_protocol_str(),
                "Maven reactor module selection and workspace relationships are not resolved",
            ),
            _ => {}
        }
    }
    if name == "plugin" && stack.iter().any(|element| element == "build") {
        add_marker(
            markers,
            "unsupported_maven_build_plugins",
            UnknownReasonCode::InsufficientSupport.as_protocol_str(),
            "Maven build plugins, extensions, lifecycle execution, and generated outputs are not analyzed",
        );
    }
}

fn add_marker(
    markers: &mut BTreeMap<String, UnknownMarker>,
    kind: &str,
    reason: &'static str,
    note: &'static str,
) {
    markers
        .entry(kind.to_string())
        .or_insert(UnknownMarker { reason, note });
}

fn candidate_to_record(
    candidate: PomDependencyCandidate,
    unit: &CodeUnit,
    markers: &mut BTreeMap<String, UnknownMarker>,
) -> Result<Option<(String, DependencyRecord)>, ParseError> {
    if !candidate.duplicate_fields.is_empty() {
        add_marker(
            markers,
            "conflicting_maven_dependency_fields",
            UnknownReasonCode::ConflictingFacts.as_protocol_str(),
            "Maven dependencies with duplicate coordinate fields were omitted",
        );
        return Ok(None);
    }
    if candidate.unsupported_nested_shape {
        add_marker(
            markers,
            "partial_maven_dependency_shape",
            UnknownReasonCode::InsufficientSupport.as_protocol_str(),
            "Nested Maven dependency metadata such as exclusions is not interpreted",
        );
    }

    let (Some(group_id), Some(artifact_id)) = (
        candidate.fields.get("groupId"),
        candidate.fields.get("artifactId"),
    ) else {
        add_marker(
            markers,
            "partial_maven_dependency_coordinates",
            UnknownReasonCode::MissingProjectConfig.as_protocol_str(),
            "Maven dependencies without static groupId and artifactId were omitted",
        );
        return Ok(None);
    };
    if !is_maven_coordinate_part(group_id) || !is_maven_coordinate_part(artifact_id) {
        add_marker(
            markers,
            "partial_maven_dependency_coordinates",
            UnknownReasonCode::MissingProjectConfig.as_protocol_str(),
            "Maven dependencies with dynamic or unsupported coordinate text were omitted",
        );
        return Ok(None);
    }

    let optional = match candidate.fields.get("optional").map(String::as_str) {
        None | Some("false") => false,
        Some("true") => true,
        Some(_) => {
            add_marker(
                markers,
                "partial_maven_optional_flag",
                UnknownReasonCode::MissingProjectConfig.as_protocol_str(),
                "Maven dependencies with a non-literal optional flag were omitted",
            );
            return Ok(None);
        }
    };

    let scope = match candidate.fields.get("scope").map(String::as_str) {
        None | Some("compile") | Some("runtime") => DependencyScope::Runtime,
        Some("test") => DependencyScope::Test,
        Some("provided" | "system" | "import") => {
            add_marker(
                markers,
                "unmapped_maven_dependency_scope",
                UnknownReasonCode::BuildVariantAmbiguity.as_protocol_str(),
                "Maven provided, system, and import scopes do not map to a proved runtime relation",
            );
            DependencyScope::Unknown
        }
        Some(_) => {
            add_marker(
                markers,
                "unmapped_maven_dependency_scope",
                UnknownReasonCode::MissingProjectConfig.as_protocol_str(),
                "Unknown or interpolated Maven dependency scope remains unresolved",
            );
            DependencyScope::Unknown
        }
    };

    if candidate.fields.contains_key("type") || candidate.fields.contains_key("classifier") {
        add_marker(
            markers,
            "unresolved_maven_artifact_variant",
            UnknownReasonCode::InsufficientSupport.as_protocol_str(),
            "Maven artifact type and classifier variants are retained only as unresolved context",
        );
    }

    let requirement = match candidate.fields.get("version") {
        Some(version) if is_static_maven_version(version) => {
            Some(DependencyVersion::new(version.clone()).map_err(ParseError::Internal)?)
        }
        Some(_) => {
            add_marker(
                markers,
                "unresolved_maven_dependency_version",
                UnknownReasonCode::MissingProjectConfig.as_protocol_str(),
                "Maven dependency versions using interpolation or unsupported text remain unresolved",
            );
            None
        }
        None => {
            add_marker(
                markers,
                "unresolved_maven_dependency_version",
                UnknownReasonCode::MissingProjectConfig.as_protocol_str(),
                "Maven dependency version may be inherited or managed and remains unresolved",
            );
            None
        }
    };

    let package_name = format!("{group_id}:{artifact_id}");
    let range =
        SourceRange::new(candidate.start_byte, candidate.end_byte).map_err(ParseError::Internal)?;
    let record = DependencyRecord::new(
        PackageIdentity::new(DependencyEcosystem::Maven, package_name.clone())
            .map_err(ParseError::Internal)?,
        requirement,
        None,
        scope,
        optional,
        DependencyDirectness::Direct,
        DependencyEvidenceLevel::ManifestDeclared,
        Evidence::new(
            unit.id.clone(),
            range,
            unit.provenance.clone(),
            "bounded static pom.xml direct dependency declaration; effective model, resolution, classpath, installation, and runtime selection remain unproved",
        )
        .map_err(ParseError::Internal)?,
    )
    .map_err(ParseError::Internal)?;
    Ok(Some((package_name, record)))
}

fn is_maven_coordinate_part(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAVEN_COORDINATE_PART_LIMIT
        && value.trim() == value
        && !value.contains("${")
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn is_static_maven_version(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAVEN_VERSION_LIMIT
        && value.trim() == value
        && !value.contains("${")
        && value.is_ascii()
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(
                    byte,
                    b'.' | b'_' | b'-' | b'+' | b',' | b'[' | b']' | b'(' | b')'
                )
        })
}

fn failed_inventory(
    unit: &CodeUnit,
    reason: &str,
    kind: &str,
    note: &str,
) -> Result<(Vec<SemanticFact>, Vec<DependencyRecord>), ParseError> {
    Ok((vec![maven_unknown(unit, reason, kind, note)?], Vec::new()))
}

fn maven_unknown(
    unit: &CodeUnit,
    reason: &str,
    kind: &str,
    note: &str,
) -> Result<SemanticFact, ParseError> {
    Ok(SemanticFact {
        kind: SemanticFactKind::Unknown,
        subject: unit.id.as_str().to_string(),
        target: Some(SymbolId::new(reason.to_string()).map_err(ParseError::Internal)?),
        origin: FactOrigin {
            engine: MAVEN_CONFIG_ENGINE.to_string(),
            engine_version: env!("CARGO_PKG_VERSION").to_string(),
            method: MAVEN_CONFIG_METHOD.to_string(),
        },
        certainty: FactCertainty::Unknown,
        evidence: Evidence::new(
            unit.id.clone(),
            unit.range.clone(),
            unit.provenance.clone(),
            note,
        )
        .map_err(ParseError::Internal)?,
        assumptions: vec![
            "affected_claim=java_dependency_inventory".to_string(),
            format!("java_unknown_kind={kind}"),
        ],
    })
}

fn stack_is(stack: &[String], expected: &[&str]) -> bool {
    stack.len() == expected.len()
        && stack
            .iter()
            .zip(expected)
            .all(|(actual, expected)| actual == expected)
}

#[derive(Debug)]
enum XmlEvent {
    Start {
        name: String,
        start_byte: usize,
        end_byte: usize,
        self_closing: bool,
    },
    End {
        name: String,
        end_byte: usize,
    },
    Text {
        text: String,
    },
}

fn xml_events(text: &str) -> Result<Vec<XmlEvent>, PomFailure> {
    let bytes = text.as_bytes();
    let mut events = Vec::new();
    let mut cursor = 0usize;
    while cursor < bytes.len() {
        if events.len() >= MAVEN_XML_TOKEN_LIMIT {
            return Err(PomFailure::ResourceLimit);
        }
        if bytes[cursor] != b'<' {
            let end = bytes[cursor..]
                .iter()
                .position(|byte| *byte == b'<')
                .map_or(bytes.len(), |offset| cursor + offset);
            events.push(XmlEvent::Text {
                text: text[cursor..end].to_string(),
            });
            cursor = end;
            continue;
        }
        if text[cursor..].starts_with("<!--") {
            let Some(offset) = text[cursor + 4..].find("-->") else {
                return Err(PomFailure::Malformed);
            };
            if text[cursor + 4..cursor + 4 + offset].contains("--") {
                return Err(PomFailure::Malformed);
            }
            cursor = cursor + 4 + offset + 3;
            continue;
        }
        if text[cursor..].starts_with("<?") {
            let Some(offset) = text[cursor + 2..].find("?>") else {
                return Err(PomFailure::Malformed);
            };
            cursor = cursor + 2 + offset + 2;
            continue;
        }
        if text[cursor..].starts_with("<!") {
            return Err(PomFailure::UnsupportedXml);
        }

        let end = xml_tag_end(text, cursor + 1)?;
        let raw = text[cursor + 1..end].trim();
        if raw.is_empty() {
            return Err(PomFailure::Malformed);
        }
        if let Some(raw_name) = raw.strip_prefix('/') {
            let name = raw_name.trim();
            if name.len() > MAVEN_XML_NAME_LIMIT {
                return Err(PomFailure::ResourceLimit);
            }
            if name.split_whitespace().count() != 1 || !is_xml_element_name(name) {
                return Err(PomFailure::Malformed);
            }
            events.push(XmlEvent::End {
                name: name.to_string(),
                end_byte: end + 1,
            });
        } else {
            let self_closing = raw.ends_with('/');
            let body = raw.strip_suffix('/').unwrap_or(raw).trim_end();
            let name_end = body.find(char::is_whitespace).unwrap_or(body.len());
            let name = &body[..name_end];
            if name.len() > MAVEN_XML_NAME_LIMIT {
                return Err(PomFailure::ResourceLimit);
            }
            if !is_xml_element_name(name) {
                return Err(if name.contains(':') {
                    PomFailure::UnsupportedXml
                } else {
                    PomFailure::Malformed
                });
            }
            validate_xml_attributes(&body[name_end..])?;
            events.push(XmlEvent::Start {
                name: name.to_string(),
                start_byte: cursor,
                end_byte: end + 1,
                self_closing,
            });
        }
        cursor = end + 1;
    }
    Ok(events)
}

fn xml_tag_end(text: &str, start: usize) -> Result<usize, PomFailure> {
    let bytes = text.as_bytes();
    let mut cursor = start;
    let mut quote = None::<u8>;
    while cursor < bytes.len() {
        match bytes[cursor] {
            b'\'' | b'"' if quote.is_none() => quote = Some(bytes[cursor]),
            value if quote == Some(value) => quote = None,
            b'<' if quote.is_none() => return Err(PomFailure::Malformed),
            b'>' if quote.is_none() => return Ok(cursor),
            _ => {}
        }
        cursor += 1;
    }
    Err(PomFailure::Malformed)
}

fn is_xml_element_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAVEN_XML_NAME_LIMIT
        && name.is_ascii()
        && name
            .as_bytes()
            .first()
            .is_some_and(|byte| byte.is_ascii_alphabetic() || *byte == b'_')
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
}

fn validate_xml_attributes(attributes: &str) -> Result<(), PomFailure> {
    let mut cursor = 0usize;
    let bytes = attributes.as_bytes();
    let mut names = BTreeSet::new();
    while cursor < bytes.len() {
        while bytes.get(cursor).is_some_and(u8::is_ascii_whitespace) {
            cursor += 1;
        }
        if cursor == bytes.len() {
            return Ok(());
        }
        let name_start = cursor;
        while bytes.get(cursor).is_some_and(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b':')
        }) {
            cursor += 1;
        }
        if cursor - name_start > MAVEN_XML_NAME_LIMIT {
            return Err(PomFailure::ResourceLimit);
        }
        if cursor == name_start {
            return Err(PomFailure::Malformed);
        }
        if !names.insert(&attributes[name_start..cursor]) {
            return Err(PomFailure::Malformed);
        }
        while bytes.get(cursor).is_some_and(u8::is_ascii_whitespace) {
            cursor += 1;
        }
        if bytes.get(cursor) != Some(&b'=') {
            return Err(PomFailure::Malformed);
        }
        cursor += 1;
        while bytes.get(cursor).is_some_and(u8::is_ascii_whitespace) {
            cursor += 1;
        }
        let Some(quote @ (b'\'' | b'"')) = bytes.get(cursor).copied() else {
            return Err(PomFailure::Malformed);
        };
        cursor += 1;
        let Some(offset) = bytes[cursor..].iter().position(|byte| *byte == quote) else {
            return Err(PomFailure::Malformed);
        };
        if bytes[cursor..cursor + offset].contains(&b'<') {
            return Err(PomFailure::Malformed);
        }
        cursor += offset + 1;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::{ContentHash, RepositoryRevision};

    fn parse(text: &str, path: &str) -> SourceParseOutput {
        parse_output(SourceDocument {
            path,
            language: Language::JavaConfig,
            content_hash: ContentHash::new(
                "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            )
            .expect("valid hash"),
            repository_revision: RepositoryRevision::new("UNKNOWN").expect("valid revision"),
            text,
        })
        .expect("parse Maven config")
    }

    fn unknowns(output: &SourceParseOutput) -> Vec<(String, String)> {
        output
            .report
            .semantic_facts
            .iter()
            .map(|fact| {
                (
                    fact.target.as_ref().expect("reason").as_str().to_string(),
                    fact.assumptions
                        .iter()
                        .find_map(|item| item.strip_prefix("java_unknown_kind="))
                        .expect("typed Java UNKNOWN")
                        .to_string(),
                )
            })
            .collect()
    }

    #[test]
    fn pom_emits_bounded_direct_dependencies_without_effective_model_claims() {
        let output = parse(
            r#"<?xml version="1.0"?>
<project xmlns="http://maven.apache.org/POM/4.0.0">
  <dependencies>
    <dependency><groupId>org.junit.jupiter</groupId><artifactId>junit-jupiter</artifactId><version>5.12.1</version><scope>test</scope></dependency>
    <dependency><groupId>com.google.guava</groupId><artifactId>guava</artifactId><version>[33.0,34.0)</version><optional>true</optional></dependency>
  </dependencies>
</project>"#,
            "nested/pom.xml",
        );

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
                    dependency.scope,
                    dependency.optional,
                    dependency.directness,
                    dependency.evidence_level,
                ))
                .collect::<Vec<_>>(),
            vec![
                (
                    "com.google.guava:guava",
                    Some("[33.0,34.0)"),
                    DependencyScope::Runtime,
                    true,
                    DependencyDirectness::Direct,
                    DependencyEvidenceLevel::ManifestDeclared,
                ),
                (
                    "org.junit.jupiter:junit-jupiter",
                    Some("5.12.1"),
                    DependencyScope::Test,
                    false,
                    DependencyDirectness::Direct,
                    DependencyEvidenceLevel::ManifestDeclared,
                ),
            ]
        );
        assert!(unknowns(&output).is_empty());
        assert!(output.dependencies.iter().all(|dependency| {
            dependency.resolved_version.is_none()
                && dependency.evidence.range.start_byte < dependency.evidence.range.end_byte
        }));
    }

    #[test]
    fn effective_model_inputs_are_explicit_unknown_and_not_promoted() {
        let output = parse(
            r#"<project>
  <parent><groupId>example</groupId><artifactId>parent</artifactId><version>1</version></parent>
  <properties><lib.version>2</lib.version></properties>
  <dependencyManagement><dependencies><dependency><groupId>managed</groupId><artifactId>bom</artifactId><version>1</version></dependency></dependencies></dependencyManagement>
  <profiles><profile><dependencies><dependency><groupId>profile</groupId><artifactId>only</artifactId><version>1</version></dependency></dependencies></profile></profiles>
  <modules><module>child</module></modules>
  <build><plugins><plugin><groupId>plugin</groupId><artifactId>generator</artifactId></plugin></plugins></build>
  <dependencies><dependency><groupId>direct</groupId><artifactId>lib</artifactId><version>${lib.version}</version><scope>provided</scope><classifier>jdk17</classifier><exclusions><exclusion><groupId>x</groupId><artifactId>y</artifactId></exclusion></exclusions></dependency></dependencies>
</project>"#,
            "pom.xml",
        );

        assert_eq!(output.dependencies.len(), 1);
        let dependency = &output.dependencies[0];
        assert_eq!(dependency.package.name, "direct:lib");
        assert!(dependency.requirement.is_none());
        assert_eq!(dependency.scope, DependencyScope::Unknown);
        let kinds = unknowns(&output)
            .into_iter()
            .map(|(_, kind)| kind)
            .collect::<BTreeSet<_>>();
        for expected in [
            "partial_maven_dependency_shape",
            "unmapped_maven_dependency_scope",
            "unresolved_maven_artifact_variant",
            "unresolved_maven_dependency_management",
            "unresolved_maven_dependency_version",
            "unresolved_maven_parent_model",
            "unresolved_maven_profile_selection",
            "unresolved_maven_properties",
            "unresolved_maven_reactor_modules",
            "unsupported_maven_build_plugins",
        ] {
            assert!(kinds.contains(expected), "missing {expected}");
        }
    }

    #[test]
    fn malformed_doctype_and_prefixed_elements_fail_closed() {
        for (text, expected) in [
            ("<project><dependencies></project>", "malformed_maven_pom"),
            ("<project xmlns='one' xmlns='two'/>", "malformed_maven_pom"),
            (
                "<project><!-- bad--comment --></project>",
                "malformed_maven_pom",
            ),
            (
                "<!DOCTYPE project [<!ENTITY x SYSTEM 'file:///etc/passwd'>]><project/>",
                "unsupported_maven_xml_construct",
            ),
            (
                "<m:project><m:dependencies/></m:project>",
                "unsupported_maven_xml_construct",
            ),
        ] {
            let output = parse(text, "pom.xml");
            assert!(output.dependencies.is_empty());
            assert_eq!(unknowns(&output)[0].1, expected);
        }
    }

    #[test]
    fn duplicate_and_incomplete_coordinates_are_omitted() {
        let output = parse(
            r#"<project><dependencies>
<dependency><groupId>a</groupId><artifactId>x</artifactId><version>1</version></dependency>
<dependency><groupId>a</groupId><artifactId>x</artifactId><version>2</version></dependency>
<dependency><groupId>a</groupId><artifactId>x</artifactId><version>3</version></dependency>
<dependency><groupId>a</groupId><groupId>b</groupId><artifactId>dup</artifactId></dependency>
<dependency><artifactId>missing-group</artifactId></dependency>
<dependency><groupId>${dynamic}</groupId><artifactId>dynamic</artifactId></dependency>
</dependencies></project>"#,
            "pom.xml",
        );
        assert!(output.dependencies.is_empty());
        let kinds = unknowns(&output)
            .into_iter()
            .map(|(_, kind)| kind)
            .collect::<BTreeSet<_>>();
        assert!(kinds.contains("conflicting_maven_dependency_declarations"));
        assert!(kinds.contains("conflicting_maven_dependency_fields"));
        assert!(kinds.contains("partial_maven_dependency_coordinates"));
    }

    #[test]
    fn dependency_limit_is_exact_and_fail_closed() {
        let dependencies = (0..MAVEN_DEPENDENCY_LIMIT)
            .map(|index| {
                format!(
                    "<dependency><groupId>g</groupId><artifactId>a{index}</artifactId><version>1</version></dependency>"
                )
            })
            .collect::<String>();
        let output = parse(
            &format!("<project><dependencies>{dependencies}</dependencies></project>"),
            "pom.xml",
        );
        assert_eq!(output.dependencies.len(), MAVEN_DEPENDENCY_LIMIT);

        let output = parse(
            &format!(
                "<project><dependencies>{dependencies}<dependency><groupId>g</groupId><artifactId>overflow</artifactId></dependency></dependencies></project>"
            ),
            "pom.xml",
        );
        assert!(output.dependencies.is_empty());
        assert_eq!(
            unknowns(&output),
            vec![(
                "ResourceLimit".to_string(),
                "maven_pom_resource_limit".to_string()
            )]
        );
    }

    #[test]
    fn maven_pom_resource_boundaries_are_inclusive_and_fail_closed() {
        let wrapper_bytes = "<project></project>".len();
        let at_bytes = format!(
            "<project>{}</project>",
            " ".repeat(
                usize::try_from(DEFAULT_MAX_FILE_BYTES).expect("file limit fits usize")
                    - wrapper_bytes
            )
        );
        assert_eq!(
            at_bytes.len(),
            usize::try_from(DEFAULT_MAX_FILE_BYTES).expect("file limit fits usize")
        );
        assert!(unknowns(&parse(&at_bytes, "pom.xml")).is_empty());
        let over_bytes = format!("{at_bytes} ");
        assert_eq!(
            unknowns(&parse(&over_bytes, "pom.xml")),
            vec![(
                "ResourceLimit".to_string(),
                "maven_pom_resource_limit".to_string()
            )]
        );

        let at_depth = format!(
            "<project>{}{}</project>",
            "<x>".repeat(MAVEN_XML_DEPTH_LIMIT - 1),
            "</x>".repeat(MAVEN_XML_DEPTH_LIMIT - 1)
        );
        assert!(unknowns(&parse(&at_depth, "pom.xml")).is_empty());
        let over_depth = format!(
            "<project>{}{}</project>",
            "<x>".repeat(MAVEN_XML_DEPTH_LIMIT),
            "</x>".repeat(MAVEN_XML_DEPTH_LIMIT)
        );
        assert_eq!(
            unknowns(&parse(&over_depth, "pom.xml")),
            vec![(
                "ResourceLimit".to_string(),
                "maven_pom_resource_limit".to_string()
            )]
        );

        let at_tokens = format!(
            "<project>{}</project>",
            "<x/>".repeat(MAVEN_XML_TOKEN_LIMIT - 2)
        );
        assert!(unknowns(&parse(&at_tokens, "pom.xml")).is_empty());
        let over_tokens = format!(
            "<project>{}</project>",
            "<x/>".repeat(MAVEN_XML_TOKEN_LIMIT - 1)
        );
        assert_eq!(
            unknowns(&parse(&over_tokens, "pom.xml")),
            vec![(
                "ResourceLimit".to_string(),
                "maven_pom_resource_limit".to_string()
            )]
        );

        let exact_name = format!("a{}", "b".repeat(MAVEN_XML_NAME_LIMIT - 1));
        assert!(unknowns(&parse(
            &format!("<project><{exact_name}/></project>"),
            "pom.xml"
        ))
        .is_empty());
        let over_name = format!("a{}", "b".repeat(MAVEN_XML_NAME_LIMIT));
        assert_eq!(
            unknowns(&parse(
                &format!("<project><{over_name}/></project>"),
                "pom.xml"
            )),
            vec![(
                "ResourceLimit".to_string(),
                "maven_pom_resource_limit".to_string()
            )]
        );

        let exact_field = "a".repeat(MAVEN_FIELD_TEXT_LIMIT);
        let exact_output = parse(
            &format!(
                "<project><dependencies><dependency><groupId>{exact_field}</groupId><artifactId>x</artifactId></dependency></dependencies></project>"
            ),
            "pom.xml",
        );
        assert!(exact_output.dependencies.is_empty());
        assert!(!unknowns(&exact_output)
            .iter()
            .any(|(_, kind)| kind == "maven_pom_resource_limit"));
        let over_field = "a".repeat(MAVEN_FIELD_TEXT_LIMIT + 1);
        assert_eq!(
            unknowns(&parse(
                &format!(
                    "<project><dependencies><dependency><groupId>{over_field}</groupId><artifactId>x</artifactId></dependency></dependencies></project>"
                ),
                "pom.xml"
            )),
            vec![(
                "ResourceLimit".to_string(),
                "maven_pom_resource_limit".to_string()
            )]
        );
    }

    #[test]
    fn output_is_deterministic_and_rejects_non_pom_inputs() {
        let text = "<project><dependencies><dependency><groupId>b</groupId><artifactId>z</artifactId><version>1</version></dependency><dependency><groupId>a</groupId><artifactId>y</artifactId><version>2</version></dependency></dependencies></project>";
        assert_eq!(parse(text, "pom.xml"), parse(text, "pom.xml"));

        let error = parse_output(SourceDocument {
            path: "build.xml",
            language: Language::JavaConfig,
            content_hash: ContentHash::new(
                "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            )
            .expect("valid hash"),
            repository_revision: RepositoryRevision::new("UNKNOWN").expect("valid revision"),
            text: "<project/>",
        });
        assert_eq!(error, Err(ParseError::UnsupportedLanguage));
    }
}
