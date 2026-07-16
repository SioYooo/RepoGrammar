//! Conservative xUnit `MemberData` identity resolution.
//!
//! This module proves only the source-member link. It never evaluates the
//! provider or claims that its returned rows match the theory signature.

use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MemberSourceKind {
    Field,
    Property,
    Method,
}

impl MemberSourceKind {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Field => "field",
            Self::Property => "property",
            Self::Method => "method",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MemberSourceState {
    Exact(MemberSourceKind),
    Ineligible,
    Ambiguous,
}

#[derive(Debug, Clone, Default)]
pub(super) struct MemberDataScope {
    members: BTreeMap<String, MemberSourceState>,
    open_world: bool,
}

impl MemberDataScope {
    pub(super) fn mark_open_world(&mut self) {
        self.open_world = true;
    }

    pub(super) fn record(
        &mut self,
        name: &str,
        kind: MemberSourceKind,
        is_public_static: bool,
        conditional: bool,
    ) {
        let state = if is_public_static && !conditional {
            MemberSourceState::Exact(kind)
        } else {
            MemberSourceState::Ineligible
        };
        self.members
            .entry(name.to_string())
            .and_modify(|existing| *existing = MemberSourceState::Ambiguous)
            .or_insert(state);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ResolvedMemberSource {
    pub(super) name: String,
    pub(super) kind: MemberSourceKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct MemberDataUnknown {
    pub(super) kind: &'static str,
    pub(super) note: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum MemberDataResolution {
    Absent,
    Exact(Vec<ResolvedMemberSource>),
    Unknown(MemberDataUnknown),
}

impl MemberDataResolution {
    pub(super) fn data_shape(&self) -> &'static str {
        match self {
            Self::Absent => "theory",
            Self::Exact(_) => "member_data_exact",
            Self::Unknown(_) => "member_data_unknown",
        }
    }

    pub(super) fn exact_assumptions(&self) -> Vec<String> {
        let Self::Exact(sources) = self else {
            return Vec::new();
        };
        let mut identities = sources
            .iter()
            .map(|source| format!("{}:{}", source.name, source.kind.as_str()))
            .collect::<Vec<_>>();
        identities.sort();
        identities.dedup();
        vec![
            "xunit_member_data_link=exact_same_class_public_static".to_string(),
            format!("xunit_member_data_sources={}", identities.join("|")),
        ]
    }
}

pub(super) fn resolve<'a>(
    argument_lists: impl IntoIterator<Item = Option<&'a str>>,
    scope: &MemberDataScope,
) -> MemberDataResolution {
    let argument_lists = argument_lists.into_iter().collect::<Vec<_>>();
    if argument_lists.is_empty() {
        return MemberDataResolution::Absent;
    }
    if scope.open_world {
        return unknown(
            "xunit_member_data_open_class_scope",
            "xUnit MemberData is declared on a partial, inherited, generic, non-class, or parse-degraded scope whose full member set is not proven",
        );
    }

    let mut resolved = Vec::with_capacity(argument_lists.len());
    for arguments in argument_lists {
        let member_name = match exact_same_class_member_name(arguments) {
            Ok(member_name) => member_name,
            Err(unknown) => return MemberDataResolution::Unknown(unknown),
        };
        match scope.members.get(&member_name) {
            Some(MemberSourceState::Exact(kind)) => resolved.push(ResolvedMemberSource {
                name: member_name,
                kind: *kind,
            }),
            Some(MemberSourceState::Ambiguous) => {
                return unknown(
                    "xunit_member_data_ambiguous_source",
                    "xUnit MemberData source name matches more than one same-class declaration",
                );
            }
            Some(MemberSourceState::Ineligible) => {
                return unknown(
                    "xunit_member_data_ineligible_source",
                    "xUnit MemberData source is not an unconditional public static field, property, or method",
                );
            }
            None => {
                return unknown(
                    "xunit_member_data_missing_source",
                    "xUnit MemberData source is not visible as a same-class declaration",
                );
            }
        }
    }

    resolved.sort_by(|left, right| {
        (left.name.as_str(), left.kind.as_str()).cmp(&(right.name.as_str(), right.kind.as_str()))
    });
    resolved.dedup();
    MemberDataResolution::Exact(resolved)
}

fn exact_same_class_member_name(arguments: Option<&str>) -> Result<String, MemberDataUnknown> {
    let arguments = arguments.ok_or_else(|| {
        unknown_value(
            "xunit_member_data_missing_arguments",
            "xUnit MemberData has no source-member argument",
        )
    })?;
    let inner = super::attribute_argument_inner(arguments).ok_or_else(|| {
        unknown_value(
            "xunit_member_data_malformed_arguments",
            "xUnit MemberData arguments are malformed",
        )
    })?;

    let mut member_name = None;
    for part in super::split_top_level_commas(inner) {
        if let Some((name, _value)) = super::split_top_level_assignment(part) {
            if name.trim() == "MemberType" {
                return Err(unknown_value(
                    "xunit_member_data_external_type",
                    "xUnit MemberData explicitly selects another declaring type",
                ));
            }
            return Err(unknown_value(
                "xunit_member_data_property_arguments",
                "xUnit MemberData includes additional attribute properties outside the bounded link form",
            ));
        }
        if let Some(colon) = super::find_top_level(part, ':') {
            let name = part[..colon].trim();
            if name != "memberName" || member_name.is_some() {
                return Err(unknown_value(
                    "xunit_member_data_dynamic_arguments",
                    "xUnit MemberData constructor arguments are not the exact same-class source form",
                ));
            }
            member_name = plain_identifier_string(part[colon + 1..].trim());
            continue;
        }
        if member_name.is_some() {
            return Err(unknown_value(
                "xunit_member_data_parameterized_source",
                "xUnit MemberData passes runtime arguments to its source member",
            ));
        }
        member_name = plain_identifier_string(part);
    }

    member_name.ok_or_else(|| {
        unknown_value(
            "xunit_member_data_dynamic_source",
            "xUnit MemberData source name is not a direct identifier string literal",
        )
    })
}

fn plain_identifier_string(text: &str) -> Option<String> {
    let text = text.trim();
    if !super::single_string_literal_consumes(text) || text.len() < 2 {
        return None;
    }
    let value = &text[1..text.len() - 1];
    (!value
        .chars()
        .any(|character| matches!(character, '\\' | '"'))
        && super::is_identifier(value))
    .then(|| value.to_string())
}

fn unknown(kind: &'static str, note: &'static str) -> MemberDataResolution {
    MemberDataResolution::Unknown(unknown_value(kind, note))
}

fn unknown_value(kind: &'static str, note: &'static str) -> MemberDataUnknown {
    MemberDataUnknown { kind, note }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_literal_links_to_unique_public_static_source() {
        let mut scope = MemberDataScope::default();
        scope.record("Cases", MemberSourceKind::Property, true, false);
        assert_eq!(
            resolve([Some("(\"Cases\")")], &scope),
            MemberDataResolution::Exact(vec![ResolvedMemberSource {
                name: "Cases".to_string(),
                kind: MemberSourceKind::Property,
            }])
        );
    }

    #[test]
    fn dynamic_external_open_and_ambiguous_sources_stay_unknown() {
        let mut scope = MemberDataScope::default();
        scope.record("Cases", MemberSourceKind::Method, true, false);
        scope.record("Cases", MemberSourceKind::Method, true, false);
        assert!(matches!(
            resolve([Some("(\"Cases\")")], &scope),
            MemberDataResolution::Unknown(MemberDataUnknown {
                kind: "xunit_member_data_ambiguous_source",
                ..
            })
        ));
        assert!(matches!(
            resolve([Some("(nameof(Cases))")], &MemberDataScope::default()),
            MemberDataResolution::Unknown(MemberDataUnknown {
                kind: "xunit_member_data_dynamic_source",
                ..
            })
        ));
        assert!(matches!(
            resolve(
                [Some("(\"Cases\", MemberType = typeof(Other))")],
                &MemberDataScope::default()
            ),
            MemberDataResolution::Unknown(MemberDataUnknown {
                kind: "xunit_member_data_external_type",
                ..
            })
        ));
        assert!(matches!(
            resolve(
                [Some("(\"Cases\", DisableDiscoveryEnumeration = true)")],
                &scope
            ),
            MemberDataResolution::Unknown(MemberDataUnknown {
                kind: "xunit_member_data_property_arguments",
                ..
            })
        ));
        let mut open_scope = MemberDataScope::default();
        open_scope.record("Cases", MemberSourceKind::Field, true, false);
        open_scope.mark_open_world();
        assert!(matches!(
            resolve([Some("(\"Cases\")")], &open_scope),
            MemberDataResolution::Unknown(MemberDataUnknown {
                kind: "xunit_member_data_open_class_scope",
                ..
            })
        ));
    }

    #[test]
    fn conditional_or_non_public_sources_are_ineligible() {
        for (is_public_static, conditional) in [(false, false), (true, true)] {
            let mut scope = MemberDataScope::default();
            scope.record(
                "Cases",
                MemberSourceKind::Property,
                is_public_static,
                conditional,
            );
            assert!(matches!(
                resolve([Some("(\"Cases\")")], &scope),
                MemberDataResolution::Unknown(MemberDataUnknown {
                    kind: "xunit_member_data_ineligible_source",
                    ..
                })
            ));
        }
    }
}
