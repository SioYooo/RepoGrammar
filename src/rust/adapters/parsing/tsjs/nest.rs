use super::parts::{self, Decorator};
use super::scope_graph::{ImportKind, ScopeGraphLite};
use super::{Anchor, AnchorOutcome, UnknownAnchor};
use crate::adapters::parsing::binding::{
    Annotation, BindingSpec, ImportScope, RoleEvidence, RoleRow,
};
use crate::core::model::{CodeUnit, CodeUnitKind, SemanticFactKind, UnknownReasonCode};
use std::slice::from_ref;

const NEST_COMMON_MODULE: &str = "@nestjs/common";

/// The role a NestJS decorator proves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NestRole {
    Controller,
    Injectable,
    Module,
    Route,
}

const fn decorator(name: &'static str) -> RoleEvidence {
    RoleEvidence::Annotation {
        name,
        namespace: NEST_COMMON_MODULE,
    }
}

/// NestJS decorator roles. A role holds when one of its evidence decorators
/// is exactly bound to `@nestjs/common`; the exactness of the binding is the
/// scope graph's (an import that is not shadowed or redeclared), the role and
/// the target it proves are the row's.
const NEST_ROLES: [RoleRow<NestRole>; 4] = [
    RoleRow {
        role: NestRole::Controller,
        declaration_kinds: &[],
        evidence: &[(decorator("Controller"), "nestjs.common.Controller")],
    },
    RoleRow {
        role: NestRole::Injectable,
        declaration_kinds: &[],
        evidence: &[(decorator("Injectable"), "nestjs.common.Injectable")],
    },
    RoleRow {
        role: NestRole::Module,
        declaration_kinds: &[],
        evidence: &[(decorator("Module"), "nestjs.common.Module")],
    },
    RoleRow {
        role: NestRole::Route,
        declaration_kinds: &[],
        evidence: &[
            (decorator("Get"), "nestjs.common.Get"),
            (decorator("Post"), "nestjs.common.Post"),
            (decorator("Put"), "nestjs.common.Put"),
            (decorator("Delete"), "nestjs.common.Delete"),
            (decorator("Patch"), "nestjs.common.Patch"),
            (decorator("Head"), "nestjs.common.Head"),
            (decorator("Options"), "nestjs.common.Options"),
            (decorator("All"), "nestjs.common.All"),
        ],
    },
];

/// Decorators are written bare (`Get`), so a name binds only through an
/// exactly bound import whose namespace is opened for the lookup.
static DECORATOR_NAMES: BindingSpec = BindingSpec {
    separator: '.',
    fully_qualified_names: false,
    name_suffixes: &[""],
    type_arguments: None,
};

/// The decorator that proves `role`, the target the row assigns it, and the
/// `@nestjs/common` export it names.
struct RoleHit<'d, 'a> {
    decorator: &'d Decorator<'a>,
    target: String,
    original: String,
}

fn role_hit<'d, 'a>(
    bindings: &ScopeGraphLite,
    decorators: &'d [Decorator<'a>],
    role: NestRole,
) -> Option<RoleHit<'d, 'a>> {
    let row = NEST_ROLES.iter().find(|row| row.role == role)?;
    #[cfg(test)]
    if bindings.reading == parts::Reading::Text {
        return super::legacy_text::nest_role_hit(bindings, decorators, row).map(
            |(decorator, original)| RoleHit {
                decorator,
                target: format!("nestjs.common.{original}"),
                original,
            },
        );
    }
    let named = |decorator: &Decorator<'_>| -> Option<String> {
        let binding = bindings.imports.get(decorator.name)?;
        if binding.module != NEST_COMMON_MODULE || bindings.name_is_unsafe_at(decorator.name, 0) {
            return None;
        }
        match &binding.kind {
            ImportKind::Named(original) => Some(original.clone()),
            ImportKind::Default | ImportKind::Namespace => None,
        }
    };
    // The first decorator, in source order, that names one of the row's
    // evidence exports.
    let (decorator, original) = decorators.iter().find_map(|decorator| {
        let original = named(decorator)?;
        row.evidence
            .iter()
            .any(|(evidence, _)| match evidence {
                RoleEvidence::Annotation { name, .. } | RoleEvidence::BaseType { name, .. } => {
                    *name == original
                }
            })
            .then_some((decorator, original))
    })?;
    let mut scope = ImportScope::default();
    scope.open(NEST_COMMON_MODULE.to_string());
    let annotation = Annotation {
        name: original.clone(),
        arguments: None,
        parse_degraded: false,
    };
    let matched = scope.first_role(
        from_ref(row),
        "",
        (&DECORATOR_NAMES, from_ref(&annotation)),
        (&DECORATOR_NAMES, &[]),
    )?;
    Some(RoleHit {
        decorator,
        target: matched.target.to_string(),
        original,
    })
}

/// Bounded, file-local context: byte ranges of NestJS controller units whose
/// `@Controller` decorator resolves to an exact `@nestjs/common` import. A route
/// method only anchors when it lies inside one of these ranges.
pub(super) struct NestContext {
    controller_ranges: Vec<(usize, usize)>,
}

impl NestContext {
    pub(super) fn analyze(bindings: &ScopeGraphLite, text: &str, units: &[CodeUnit]) -> Self {
        let mut controller_ranges = Vec::new();
        for unit in units {
            if unit.kind != CodeUnitKind::NestController {
                continue;
            }
            let decorators =
                parts::decorators(bindings, text, unit.range.start_byte..unit.range.end_byte);
            if role_hit(bindings, &decorators, NestRole::Controller).is_some() {
                controller_ranges.push((unit.range.start_byte, unit.range.end_byte));
            }
        }
        Self { controller_ranges }
    }

    fn contains(&self, start_byte: usize) -> bool {
        self.controller_ranges
            .iter()
            .any(|(start, end)| *start <= start_byte && start_byte < *end)
    }
}

pub(super) fn controller_anchor(
    bindings: &ScopeGraphLite,
    decorators: &[Decorator<'_>],
) -> AnchorOutcome {
    let Some(hit) = role_hit(bindings, decorators, NestRole::Controller) else {
        return unresolved_controller_import();
    };
    let anchor = Anchor {
        target: hit.target,
        fact_kind: SemanticFactKind::Symbol,
        assumptions: vec![
            "tsjs_anchor_kind=nest_controller".to_string(),
            format!("class_route_path_shape={}", hit.decorator.path_shape),
            "support_family=nestjs.common.Controller".to_string(),
        ],
    };
    AnchorOutcome::AnchorWithSubclaims(anchor, vec![di_subclaim()])
}

pub(super) fn injectable_anchor(
    bindings: &ScopeGraphLite,
    decorators: &[Decorator<'_>],
) -> AnchorOutcome {
    let Some(hit) = role_hit(bindings, decorators, NestRole::Injectable) else {
        return unresolved_controller_import();
    };
    let anchor = Anchor {
        target: hit.target,
        fact_kind: SemanticFactKind::Symbol,
        assumptions: vec![
            "tsjs_anchor_kind=nest_injectable".to_string(),
            "support_family=nestjs.common.Injectable".to_string(),
        ],
    };
    AnchorOutcome::AnchorWithSubclaims(anchor, vec![di_subclaim()])
}

pub(super) fn module_anchor(
    bindings: &ScopeGraphLite,
    decorators: &[Decorator<'_>],
) -> AnchorOutcome {
    let Some(hit) = role_hit(bindings, decorators, NestRole::Module) else {
        return unresolved_controller_import();
    };
    let anchor = Anchor {
        target: hit.target,
        fact_kind: SemanticFactKind::Symbol,
        assumptions: vec![
            "tsjs_anchor_kind=nest_module".to_string(),
            format!("nest_module_shape={}", hit.decorator.module_shape),
            "support_family=nestjs.common.Module".to_string(),
        ],
    };
    if hit.decorator.for_root {
        return AnchorOutcome::AnchorWithSubclaims(
            anchor,
            vec![UnknownAnchor {
                reason: UnknownReasonCode::FrameworkMagic,
                affected_claim: "tsjs_nest_dynamic_module",
                kind: "tsjs_nest_dynamic_module",
                note: "NestJS dynamic module (forRoot/forRootAsync) metadata is a runtime concern",
            }],
        );
    }
    AnchorOutcome::Anchor(anchor)
}

pub(super) fn route_anchor(
    bindings: &ScopeGraphLite,
    nest_context: &NestContext,
    decorators: &[Decorator<'_>],
    start_byte: usize,
) -> AnchorOutcome {
    let Some(hit) = role_hit(bindings, decorators, NestRole::Route) else {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::UnresolvedImport,
            affected_claim: "tsjs_nest_controller_identity",
            kind: "nest_unresolved_route_import",
            note: "NestJS route decorator is not an exact @nestjs/common HTTP-method import",
        });
    };
    if !nest_context.contains(start_byte) {
        return AnchorOutcome::Unknown(UnknownAnchor {
            reason: UnknownReasonCode::FrameworkMagic,
            affected_claim: "tsjs_nest_controller_identity",
            kind: "nest_route_outside_controller",
            note: "NestJS route method is not inside an exact @nestjs/common controller",
        });
    }
    AnchorOutcome::Anchor(Anchor {
        target: hit.target,
        fact_kind: SemanticFactKind::ResolvedCall,
        assumptions: vec![
            "tsjs_anchor_kind=nest_route".to_string(),
            format!("http_method={}", hit.original.to_ascii_lowercase()),
            format!("route_path_shape={}", hit.decorator.path_shape),
        ],
    })
}

fn unresolved_controller_import() -> AnchorOutcome {
    AnchorOutcome::Unknown(UnknownAnchor {
        reason: UnknownReasonCode::UnresolvedImport,
        affected_claim: "tsjs_nest_controller_identity",
        kind: "nest_unresolved_controller_import",
        note: "NestJS class decorator is not an exact @nestjs/common import",
    })
}

fn di_subclaim() -> UnknownAnchor {
    UnknownAnchor {
        reason: UnknownReasonCode::RuntimeDependencyInjection,
        affected_claim: "tsjs_nest_di_resolution",
        kind: "tsjs_nest_di_resolution",
        note: "NestJS dependency injection token resolution is a runtime concern",
    }
}
