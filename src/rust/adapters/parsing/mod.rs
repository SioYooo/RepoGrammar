//! Parsing adapters. Tree-sitter types must not cross this module boundary.

use crate::core::model::{CodeUnit, CodeUnitKind, IrEdge, IrEdgeLabel, IrNode, IrNodeId};
use crate::ports::parser::{ParseError, ParseReport, SourceDocument, SourceParser};
use std::collections::BTreeSet;

pub mod python;
pub mod syntax;
pub mod tree_sitter;

#[derive(Debug, Default)]
pub struct RepoGrammarSourceParser {
    syntax: syntax::SyntaxCodeUnitParser,
    python: python::PythonAstParser,
}

impl SourceParser for RepoGrammarSourceParser {
    fn parse(&self, document: SourceDocument<'_>) -> Result<ParseReport, ParseError> {
        match document.language {
            crate::core::model::Language::TypeScript | crate::core::model::Language::JavaScript => {
                self.syntax.parse(document)
            }
            crate::core::model::Language::Python => self.python.parse(document),
            crate::core::model::Language::Unknown(_) => Err(ParseError::UnsupportedLanguage),
        }
    }
}

pub(crate) fn ir_nodes_for_units(units: &[CodeUnit]) -> Result<Vec<IrNode>, String> {
    let mut nodes = units
        .iter()
        .map(IrNode::from_code_unit)
        .collect::<Result<Vec<_>, _>>()?;
    nodes.sort_by(|left, right| left.id.as_str().cmp(right.id.as_str()));
    Ok(nodes)
}

pub(crate) fn ir_edges_for_units(units: &[CodeUnit]) -> Result<Vec<IrEdge>, String> {
    let mut edge_keys = BTreeSet::new();
    let module_units = units
        .iter()
        .filter(|unit| unit.kind == CodeUnitKind::Module)
        .collect::<Vec<_>>();
    let class_units = units
        .iter()
        .filter(|unit| is_class_like(unit.kind.as_str()))
        .collect::<Vec<_>>();

    for unit in units {
        if unit.kind == CodeUnitKind::Module {
            continue;
        }
        for module in &module_units {
            if same_file(module, unit) && range_contains(module, unit) {
                edge_keys.insert((
                    IrNodeId::for_code_unit(&module.id)?.as_str().to_string(),
                    IrNodeId::for_code_unit(&unit.id)?.as_str().to_string(),
                    IrEdgeLabel::Contains.as_str().to_string(),
                ));
            }
        }
        if is_method_like(unit.kind.as_str()) {
            for class_unit in &class_units {
                if same_file(class_unit, unit) && range_contains(class_unit, unit) {
                    edge_keys.insert((
                        IrNodeId::for_code_unit(&class_unit.id)?
                            .as_str()
                            .to_string(),
                        IrNodeId::for_code_unit(&unit.id)?.as_str().to_string(),
                        IrEdgeLabel::Contains.as_str().to_string(),
                    ));
                }
            }
        }
    }

    edge_keys
        .into_iter()
        .map(|(from, to, _label)| {
            IrEdge::new(
                IrNodeId::new(from)?,
                IrNodeId::new(to)?,
                IrEdgeLabel::Contains,
            )
        })
        .collect()
}

fn same_file(left: &CodeUnit, right: &CodeUnit) -> bool {
    left.provenance.path == right.provenance.path
}

fn range_contains(parent: &CodeUnit, child: &CodeUnit) -> bool {
    parent.range.start_byte <= child.range.start_byte
        && child.range.end_byte <= parent.range.end_byte
}

fn is_class_like(kind: &str) -> bool {
    matches!(kind, "class" | "pydantic_model" | "sqlalchemy_model")
}

fn is_method_like(kind: &str) -> bool {
    matches!(kind, "method" | "sqlalchemy_repository_method")
}
