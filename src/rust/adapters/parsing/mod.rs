//! Parsing adapters. Tree-sitter types must not cross this module boundary.

use crate::core::model::{CodeUnit, CodeUnitKind, IrEdge, IrEdgeLabel, IrNode, IrNodeId};
use crate::ports::parser::{
    ParseError, ParseReport, ParserProjectContext, SourceDocument, SourceParser,
};
use std::collections::BTreeSet;

pub mod python;
pub mod rust_syntax;
pub mod syntax;
pub mod tree_sitter;
pub mod tsjs_anchors;

#[derive(Debug, Default)]
pub struct RepoGrammarSourceParser {
    syntax: syntax::SyntaxCodeUnitParser,
    python: python::PythonAstParser,
    rust: rust_syntax::RustSyntaxParser,
}

impl SourceParser for RepoGrammarSourceParser {
    fn parse(&self, document: SourceDocument<'_>) -> Result<ParseReport, ParseError> {
        match document.language {
            crate::core::model::Language::TypeScript
            | crate::core::model::Language::JavaScript
            | crate::core::model::Language::TsJsConfig => self.syntax.parse(document),
            crate::core::model::Language::Python | crate::core::model::Language::PythonConfig => {
                self.python.parse(document)
            }
            crate::core::model::Language::Rust | crate::core::model::Language::RustConfig => {
                self.rust.parse(document)
            }
            crate::core::model::Language::Unknown(_) => Err(ParseError::UnsupportedLanguage),
        }
    }

    fn parse_with_context(
        &self,
        document: SourceDocument<'_>,
        context: &ParserProjectContext,
    ) -> Result<ParseReport, ParseError> {
        match document.language {
            crate::core::model::Language::TypeScript
            | crate::core::model::Language::JavaScript
            | crate::core::model::Language::TsJsConfig => {
                self.syntax.parse_with_context(document, context)
            }
            crate::core::model::Language::Python | crate::core::model::Language::PythonConfig => {
                self.python.parse_with_context(document, context)
            }
            crate::core::model::Language::Rust | crate::core::model::Language::RustConfig => {
                self.rust.parse_with_context(document, context)
            }
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
    matches!(
        kind,
        "class" | "pydantic_model" | "sqlalchemy_model" | "rust_impl_block" | "rust_trait"
    )
}

fn is_method_like(kind: &str) -> bool {
    matches!(
        kind,
        "method"
            | "sqlalchemy_repository_method"
            | "rust_method"
            | "rust_trait_method"
            | "rust_associated_function"
    )
}
