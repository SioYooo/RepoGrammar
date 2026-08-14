//! Parsing adapters. Tree-sitter types must not cross this module boundary.

use crate::core::model::{CodeUnit, CodeUnitKind, IrEdge, IrEdgeLabel, IrNode, IrNodeId};
use crate::ports::parser::{
    ParseError, ParseReport, ParserProjectContext, PythonInterfaceProbe, SourceDocument,
    SourceParseOutput, SourceParser,
};
use std::collections::BTreeSet;

pub mod ada;
pub mod assembly;
pub(crate) mod bounded_json;
pub(crate) mod bounded_xml;
pub mod cpp;
pub mod csharp;
pub mod delphi;
pub mod fortran;
pub mod go;
pub mod java;
pub mod matlab;
pub mod php;
pub mod python;
pub mod r;
pub mod ruby;
pub mod rust;
pub mod sql;
pub mod swift;
pub mod syntax;
pub mod tree_sitter;
pub mod tsjs;
pub mod visual_basic;

#[derive(Debug, Default)]
pub struct RepoGrammarSourceParser {
    syntax: syntax::SyntaxCodeUnitParser,
    python: python::PythonAstParser,
    java: java::JavaSyntaxParser,
    java_config: java::maven::JavaMavenConfigParser,
    matlab_config: matlab::MatlabPackageConfigParser,
    assembly: assembly::AssemblySyntaxParser,
    csharp: csharp::CSharpSyntaxParser,
    delphi: delphi::DelphiProjectConfigParser,
    cpp: cpp::CppSyntaxParser,
    go: go::GoProjectConfigParser,
    php: php::PhpConfigParser,
    ruby: RubyConfigParser,
    r: r::RProjectConfigParser,
    go_source: go::source::GoTestSourceParser,
    rust: rust::RustSyntaxParser,
    sql: sql::SqlDdlParser,
    swift: swift::SwiftProjectConfigParser,
    visual_basic: visual_basic::VisualBasicProjectConfigParser,
    ada: ada::AdaProjectConfigParser,
    fortran: fortran::FortranProjectConfigParser,
}

#[derive(Debug, Default)]
struct RubyConfigParser;

impl SourceParser for RubyConfigParser {
    fn parse(&self, document: SourceDocument<'_>) -> Result<ParseReport, ParseError> {
        ruby::parse(document)
    }

    fn parse_with_context(
        &self,
        document: SourceDocument<'_>,
        _context: &ParserProjectContext,
    ) -> Result<ParseReport, ParseError> {
        ruby::parse(document)
    }

    fn parse_with_context_output(
        &self,
        document: SourceDocument<'_>,
        _context: &ParserProjectContext,
    ) -> Result<SourceParseOutput, ParseError> {
        ruby::parse_output(document)
    }
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
            crate::core::model::Language::Java => self.java.parse(document),
            crate::core::model::Language::JavaConfig => self.java_config.parse(document),
            crate::core::model::Language::Matlab => Err(ParseError::UnsupportedLanguage),
            crate::core::model::Language::MatlabConfig => self.matlab_config.parse(document),
            crate::core::model::Language::Assembly => self.assembly.parse(document),
            crate::core::model::Language::CSharp => self.csharp.parse(document),
            crate::core::model::Language::C
            | crate::core::model::Language::Cpp
            | crate::core::model::Language::CppConfig => self.cpp.parse(document),
            crate::core::model::Language::Go => self.go_source.parse(document),
            crate::core::model::Language::GoConfig => self.go.parse(document),
            crate::core::model::Language::PhpConfig => self.php.parse(document),
            crate::core::model::Language::Php
            | crate::core::model::Language::Ruby
            | crate::core::model::Language::Swift => Err(ParseError::UnsupportedLanguage),
            crate::core::model::Language::SwiftConfig => self.swift.parse(document),
            crate::core::model::Language::VisualBasicConfig => self.visual_basic.parse(document),
            crate::core::model::Language::VisualBasic
            | crate::core::model::Language::ObjectPascal => Err(ParseError::UnsupportedLanguage),
            crate::core::model::Language::DelphiConfig => self.delphi.parse(document),
            crate::core::model::Language::Ada => Err(ParseError::UnsupportedLanguage),
            crate::core::model::Language::AdaConfig => self.ada.parse(document),
            crate::core::model::Language::Fortran => Err(ParseError::UnsupportedLanguage),
            crate::core::model::Language::FortranConfig => self.fortran.parse(document),
            crate::core::model::Language::RubyConfig => self.ruby.parse(document),
            crate::core::model::Language::RConfig => self.r.parse(document),
            crate::core::model::Language::Sql => self.sql.parse(document),
            crate::core::model::Language::R => Err(ParseError::UnsupportedLanguage),
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
            crate::core::model::Language::Java => self.java.parse_with_context(document, context),
            crate::core::model::Language::JavaConfig => {
                self.java_config.parse_with_context(document, context)
            }
            crate::core::model::Language::Matlab => Err(ParseError::UnsupportedLanguage),
            crate::core::model::Language::MatlabConfig => {
                self.matlab_config.parse_with_context(document, context)
            }
            crate::core::model::Language::Assembly => {
                self.assembly.parse_with_context(document, context)
            }
            crate::core::model::Language::CSharp => {
                self.csharp.parse_with_context(document, context)
            }
            crate::core::model::Language::C
            | crate::core::model::Language::Cpp
            | crate::core::model::Language::CppConfig => {
                self.cpp.parse_with_context(document, context)
            }
            crate::core::model::Language::Go => {
                self.go_source.parse_with_context(document, context)
            }
            crate::core::model::Language::GoConfig => self.go.parse_with_context(document, context),
            crate::core::model::Language::PhpConfig => {
                self.php.parse_with_context(document, context)
            }
            crate::core::model::Language::Php
            | crate::core::model::Language::Ruby
            | crate::core::model::Language::Swift => Err(ParseError::UnsupportedLanguage),
            crate::core::model::Language::SwiftConfig => {
                self.swift.parse_with_context(document, context)
            }
            crate::core::model::Language::VisualBasicConfig => {
                self.visual_basic.parse_with_context(document, context)
            }
            crate::core::model::Language::VisualBasic
            | crate::core::model::Language::ObjectPascal => Err(ParseError::UnsupportedLanguage),
            crate::core::model::Language::DelphiConfig => {
                self.delphi.parse_with_context(document, context)
            }
            crate::core::model::Language::Ada => Err(ParseError::UnsupportedLanguage),
            crate::core::model::Language::AdaConfig => {
                self.ada.parse_with_context(document, context)
            }
            crate::core::model::Language::Fortran => Err(ParseError::UnsupportedLanguage),
            crate::core::model::Language::FortranConfig => {
                self.fortran.parse_with_context(document, context)
            }
            crate::core::model::Language::RubyConfig => {
                self.ruby.parse_with_context(document, context)
            }
            crate::core::model::Language::RConfig => self.r.parse_with_context(document, context),
            crate::core::model::Language::Sql => self.sql.parse_with_context(document, context),
            crate::core::model::Language::R => Err(ParseError::UnsupportedLanguage),
            crate::core::model::Language::Rust | crate::core::model::Language::RustConfig => {
                self.rust.parse_with_context(document, context)
            }
            crate::core::model::Language::Unknown(_) => Err(ParseError::UnsupportedLanguage),
        }
    }

    fn parse_with_context_output(
        &self,
        document: SourceDocument<'_>,
        context: &ParserProjectContext,
    ) -> Result<SourceParseOutput, ParseError> {
        match document.language {
            crate::core::model::Language::TypeScript
            | crate::core::model::Language::JavaScript
            | crate::core::model::Language::TsJsConfig => {
                self.syntax.parse_with_context_output(document, context)
            }
            crate::core::model::Language::Python | crate::core::model::Language::PythonConfig => {
                self.python.parse_with_context_output(document, context)
            }
            crate::core::model::Language::JavaConfig => self
                .java_config
                .parse_with_context_output(document, context),
            crate::core::model::Language::MatlabConfig => self
                .matlab_config
                .parse_with_context_output(document, context),
            crate::core::model::Language::Assembly => {
                self.assembly.parse_with_context_output(document, context)
            }
            crate::core::model::Language::C
            | crate::core::model::Language::Cpp
            | crate::core::model::Language::CppConfig => {
                self.cpp.parse_with_context_output(document, context)
            }
            crate::core::model::Language::Go => {
                self.go_source.parse_with_context_output(document, context)
            }
            crate::core::model::Language::GoConfig => {
                self.go.parse_with_context_output(document, context)
            }
            crate::core::model::Language::PhpConfig => {
                self.php.parse_with_context_output(document, context)
            }
            crate::core::model::Language::SwiftConfig => {
                self.swift.parse_with_context_output(document, context)
            }
            crate::core::model::Language::VisualBasicConfig => self
                .visual_basic
                .parse_with_context_output(document, context),
            crate::core::model::Language::DelphiConfig => {
                self.delphi.parse_with_context_output(document, context)
            }
            crate::core::model::Language::AdaConfig => {
                self.ada.parse_with_context_output(document, context)
            }
            crate::core::model::Language::FortranConfig => {
                self.fortran.parse_with_context_output(document, context)
            }
            crate::core::model::Language::RubyConfig => {
                self.ruby.parse_with_context_output(document, context)
            }
            crate::core::model::Language::RConfig => {
                self.r.parse_with_context_output(document, context)
            }
            crate::core::model::Language::Sql => {
                self.sql.parse_with_context_output(document, context)
            }
            _ => self
                .parse_with_context(document, context)
                .map(SourceParseOutput::from_report),
        }
    }

    fn extract_python_interface(&self, path: &str, text: &str) -> PythonInterfaceProbe {
        // Only the Python frontend computes an interface; the preflight only ever
        // probes discovered `.py` modules, so every other language keeps the
        // conservative `Unverified` default.
        self.python.extract_python_interface(path, text)
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
        .filter(|unit| is_module_like(&unit.kind))
        .collect::<Vec<_>>();
    let class_units = units
        .iter()
        .filter(|unit| is_class_like(unit.kind.as_str()))
        .collect::<Vec<_>>();

    for unit in units {
        if is_module_like(&unit.kind) {
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

fn is_module_like(kind: &CodeUnitKind) -> bool {
    matches!(
        kind,
        CodeUnitKind::Module | CodeUnitKind::RustModule | CodeUnitKind::RustInlineModule
    )
}

fn is_class_like(kind: &str) -> bool {
    matches!(
        kind,
        "class"
            | "pydantic_model"
            | "sqlalchemy_model"
            | "spring_component"
            | "spring_boot_application"
            | "spring_data_repository"
            | "jpa_entity"
            | "jpa_mapped_superclass"
            | "jpa_embeddable"
            | "jaxrs_resource_class"
            | "servlet_http_servlet"
            | "marshmallow_schema"
            | "aspnet_controller"
            | "efcore_db_context"
            | "fluentvalidation_validator"
            | "gtest_test_fixture"
            | "qt_object_class"
            | "rust_impl_block"
            | "rust_trait"
    )
}

fn is_method_like(kind: &str) -> bool {
    matches!(
        kind,
        "method"
            | "sqlalchemy_repository_method"
            | "spring_mvc_route"
            | "aspnet_controller_action"
            | "aspnet_minimal_api_route"
            | "efcore_entity_set"
            | "xunit_test_method"
            | "nunit_test_method"
            | "mstest_test_method"
            | "gtest_test_case"
            | "catch2_test_case"
            | "doctest_test_case"
            | "boost_test_case"
            | "boost_test_suite"
            | "cppunit_suite_registration"
            | "junit5_test_method"
            | "junit4_test_method"
            | "testng_test_method"
            | "jaxrs_resource_method"
            | "rust_method"
            | "rust_trait_method"
            | "rust_associated_function"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::{
        ContentHash, FactCertainty, Language, RepositoryRevision, SemanticFactKind, SymbolId,
    };

    fn python_config_document<'a>(path: &'a str, text: &'a str) -> SourceDocument<'a> {
        SourceDocument {
            path,
            language: Language::PythonConfig,
            content_hash: ContentHash::new(
                "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            )
            .expect("valid hash"),
            repository_revision: RepositoryRevision::new("UNKNOWN").expect("valid revision"),
            text,
        }
    }

    fn go_inventory_document(language: Language) -> SourceDocument<'static> {
        SourceDocument {
            path: match language {
                Language::Go => "main.go",
                Language::GoConfig => "go.mod",
                _ => unreachable!("Go inventory helper accepts only Go tokens"),
            },
            language,
            content_hash: ContentHash::new(
                "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            )
            .expect("valid hash"),
            repository_revision: RepositoryRevision::new("UNKNOWN").expect("valid revision"),
            text: "inventory only",
        }
    }

    fn ruby_inventory_document(language: Language) -> SourceDocument<'static> {
        let text = if language == Language::RubyConfig {
            "DEPENDENCIES\n  rack\n"
        } else {
            "inventory only"
        };
        SourceDocument {
            path: match language {
                Language::Ruby => "main.rb",
                Language::RubyConfig => "Gemfile.lock",
                _ => unreachable!("Ruby inventory helper accepts only Ruby tokens"),
            },
            language,
            content_hash: ContentHash::new(
                "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            )
            .expect("valid hash"),
            repository_revision: RepositoryRevision::new("UNKNOWN").expect("valid revision"),
            text,
        }
    }

    fn php_inventory_document(language: Language) -> SourceDocument<'static> {
        SourceDocument {
            path: match language {
                Language::Php => "main.php",
                Language::PhpConfig => "composer.json",
                _ => unreachable!("PHP inventory helper accepts only PHP tokens"),
            },
            language,
            content_hash: ContentHash::new(
                "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            )
            .expect("valid hash"),
            repository_revision: RepositoryRevision::new("UNKNOWN").expect("valid revision"),
            text: "inventory only",
        }
    }

    fn swift_inventory_document(language: Language) -> SourceDocument<'static> {
        SourceDocument {
            path: match language {
                Language::Swift => "main.swift",
                Language::SwiftConfig => "Package.swift",
                _ => unreachable!("Swift inventory helper accepts only Swift tokens"),
            },
            language,
            content_hash: ContentHash::new(
                "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            )
            .expect("valid hash"),
            repository_revision: RepositoryRevision::new("UNKNOWN").expect("valid revision"),
            text: "inventory only",
        }
    }

    fn sql_r_inventory_document(language: Language) -> SourceDocument<'static> {
        let path = match &language {
            Language::Sql => "schema.sql",
            Language::R => "main.R",
            Language::RConfig => "renv.lock",
            _ => unreachable!("SQL/R inventory helper accepts only SQL/R tokens"),
        };
        let text = if language == Language::RConfig {
            r#"{"Packages":{"jsonlite":{"Package":"jsonlite","Version":"1.8.8","Source":"Repository","Repository":"CRAN"}}}"#
        } else {
            "inventory only"
        };
        SourceDocument {
            path,
            language,
            content_hash: ContentHash::new(
                "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            )
            .expect("valid hash"),
            repository_revision: RepositoryRevision::new("UNKNOWN").expect("valid revision"),
            text,
        }
    }

    fn matlab_inventory_document(language: Language) -> SourceDocument<'static> {
        SourceDocument {
            path: match language {
                Language::Matlab => "main.m",
                Language::MatlabConfig => "resources/mpackage.json",
                _ => unreachable!("MATLAB inventory helper accepts only MATLAB tokens"),
            },
            language,
            content_hash: ContentHash::new(
                "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            )
            .expect("valid hash"),
            repository_revision: RepositoryRevision::new("UNKNOWN").expect("valid revision"),
            text: r#"{"name":"Demo","version":"1.0.0","id":"af92112b-8b66-44d1-b4b1-848f54affa3e","schemaVersion":"1.1.0","dependencies":[]}"#,
        }
    }

    #[test]
    fn product_parser_rejects_go_source_but_statically_parses_go_mod() {
        let parser = RepoGrammarSourceParser::default();
        assert_eq!(
            parser.parse(go_inventory_document(Language::Go)),
            Err(ParseError::UnsupportedLanguage)
        );
        let report = parser
            .parse_with_context(
                SourceDocument {
                    text: "module example.test/app\nrequire example.test/lib v1.0.0\n",
                    ..go_inventory_document(Language::GoConfig)
                },
                &ParserProjectContext::default(),
            )
            .expect("statically parse go.mod project config");
        assert_eq!(report.units.len(), 1);
        assert_eq!(report.units[0].kind, CodeUnitKind::ProjectConfig);
    }

    #[test]
    fn product_parser_routes_sql_to_its_frontend_rejects_r_source_and_parses_exact_r_metadata() {
        let parser = RepoGrammarSourceParser::default();
        assert_eq!(
            parser.parse(sql_r_inventory_document(Language::R)),
            Err(ParseError::UnsupportedLanguage)
        );
        // ADR-0040 routes SQL to the bounded DDL frontend. "inventory only" is
        // not an admitted statement shape, so the file yields its module unit
        // and one statement unit and no anchor.
        let sql = parser
            .parse(sql_r_inventory_document(Language::Sql))
            .expect("SQL must reach the bounded DDL frontend");
        assert_eq!(sql.units.len(), 2);
        assert!(sql
            .semantic_facts
            .iter()
            .all(|fact| !fact.certainty.supports_family_membership()));
        let output = parser
            .parse_with_context_output(
                sql_r_inventory_document(Language::RConfig),
                &ParserProjectContext::default(),
            )
            .expect("exact renv.lock must return bounded dependency evidence");
        assert_eq!(output.report.units.len(), 1);
        assert_eq!(output.report.units[0].kind, CodeUnitKind::ProjectConfig);
        assert_eq!(output.dependencies.len(), 1);
        assert_eq!(
            output.dependencies[0].package.ecosystem,
            crate::core::model::DependencyEcosystem::Cran
        );
    }

    #[test]
    fn product_parser_rejects_ruby_source_but_qualifies_exact_bundler_lock() {
        let parser = RepoGrammarSourceParser::default();
        assert_eq!(
            parser.parse(ruby_inventory_document(Language::Ruby)),
            Err(ParseError::UnsupportedLanguage)
        );

        let output = parser
            .parse_with_context_output(
                ruby_inventory_document(Language::RubyConfig),
                &ParserProjectContext::default(),
            )
            .expect("exact Gemfile.lock must return bounded dependency evidence");
        assert_eq!(output.report.units.len(), 1);
        assert!(output.report.semantic_facts.is_empty());
        assert_eq!(output.dependencies.len(), 1);
        assert_eq!(output.dependencies[0].package.name.as_str(), "rack");
    }

    #[test]
    fn product_parser_rejects_php_source_but_accepts_composer_config() {
        let parser = RepoGrammarSourceParser::default();
        assert_eq!(
            parser.parse(php_inventory_document(Language::Php)),
            Err(ParseError::UnsupportedLanguage)
        );
        let report = parser
            .parse_with_context(
                php_inventory_document(Language::PhpConfig),
                &ParserProjectContext::default(),
            )
            .expect("Composer config has a bounded static parser");
        assert_eq!(report.units.len(), 1);
        assert_eq!(report.units[0].kind, CodeUnitKind::ProjectConfig);
        assert!(report
            .semantic_facts
            .iter()
            .all(|fact| fact.kind == SemanticFactKind::Unknown));
    }

    #[test]
    fn product_parser_rejects_swift_source_and_accepts_bounded_config() {
        let parser = RepoGrammarSourceParser::default();
        assert_eq!(
            parser.parse(swift_inventory_document(Language::Swift)),
            Err(ParseError::UnsupportedLanguage)
        );
        let report = parser
            .parse_with_context(
                swift_inventory_document(Language::SwiftConfig),
                &ParserProjectContext::default(),
            )
            .expect("parse bounded Swift project config");
        assert_eq!(report.units.len(), 1);
        assert_eq!(report.units[0].kind, CodeUnitKind::ProjectConfig);
    }

    #[test]
    fn product_parser_routes_matlab_config_and_assembly_without_source_execution() {
        let parser = RepoGrammarSourceParser::default();
        assert_eq!(
            parser.parse(matlab_inventory_document(Language::Matlab)),
            Err(ParseError::UnsupportedLanguage)
        );
        let matlab = parser
            .parse_with_context_output(
                matlab_inventory_document(Language::MatlabConfig),
                &ParserProjectContext::default(),
            )
            .expect("route bounded MATLAB package config");
        assert_eq!(matlab.report.units[0].kind, CodeUnitKind::ProjectConfig);
        assert!(matlab.dependencies.is_empty());

        let assembly = parser
            .parse(SourceDocument {
                path: "start.s",
                language: Language::Assembly,
                content_hash: ContentHash::new(
                    "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                )
                .expect("valid hash"),
                repository_revision: RepositoryRevision::new("UNKNOWN").expect("valid revision"),
                text: ".text\nentry:\n call helper\n",
            })
            .expect("route bounded assembly scanner");
        assert!(assembly
            .semantic_facts
            .iter()
            .all(|fact| !fact.certainty.supports_family_membership()));
    }

    #[test]
    fn product_parser_statically_parses_root_setup_py_project_config() {
        let source = r#"from setuptools import find_packages, setup

open("product-parser-setup-py-sentinel", "w").write("must not execute")

setup(
    name="demo-setup-py",
    package_dir={"": "app"},
    packages=find_packages(where="app"),
)
"#;

        let report = RepoGrammarSourceParser::default()
            .parse(python_config_document("setup.py", source))
            .expect("root setup.py is parsed as static project config");

        assert_eq!(report.units.len(), 1);
        assert_eq!(report.units[0].language, Language::PythonConfig);
        assert_eq!(report.units[0].kind, CodeUnitKind::ProjectConfig);
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.target.as_ref().map(SymbolId::as_str)
                == Some("python.project_config.project_name.demo-setup-py")
        }));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.target.as_ref().map(SymbolId::as_str)
                == Some("python.project_config.source_root.app")
        }));
        assert!(report.semantic_facts.iter().all(|fact| {
            fact.origin.engine == "python"
                && fact.origin.method == "cpython_ast"
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "parsed_with=cpython_ast")
        }));
        assert!(!format!("{report:?}").contains("must not execute"));
    }

    #[test]
    fn product_parser_records_the_exact_setup_cfg_frontend() {
        let report = RepoGrammarSourceParser::default()
            .parse(python_config_document(
                "setup.cfg",
                "[metadata]\nname = demo\n\n[options.packages.find]\nwhere = src\n",
            ))
            .expect("root setup.cfg is parsed as static project config");

        assert!(report.semantic_facts.iter().any(|fact| {
            fact.target.as_ref().map(SymbolId::as_str)
                == Some("python.project_config.source_root.src")
                && fact.origin.method == "configparser"
                && fact
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == "parsed_with=configparser")
        }));
    }

    #[test]
    fn product_parser_keeps_malformed_and_dynamic_setup_py_conservative() {
        let malformed = "from setuptools import setup\nsetup(\n";
        let malformed_report = RepoGrammarSourceParser::default()
            .parse(python_config_document("setup.py", malformed))
            .expect("malformed setup.py remains a typed project-config result");
        assert_eq!(malformed_report.units.len(), 1);
        assert!(malformed_report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Unknown
                && fact.certainty == FactCertainty::Unknown
                && fact.target.as_ref().map(SymbolId::as_str) == Some("MissingProjectConfig")
                && fact.origin.method == "cpython_ast"
        }));
        assert!(!format!("{malformed_report:?}").contains("setup(\n"));

        let dynamic = r#"from setuptools import find_packages, setup

def choose_root():
    return "src"

root = choose_root()
setup(name=choose_root(), package_dir={"": root}, packages=find_packages(where=root))
"#;
        let dynamic_report = RepoGrammarSourceParser::default()
            .parse(python_config_document("setup.py", dynamic))
            .expect("dynamic setup.py is parsed without executing computed values");
        assert_eq!(dynamic_report.units.len(), 1);
        assert_eq!(dynamic_report.semantic_facts.len(), 1);
        assert!(dynamic_report.semantic_facts.iter().any(|fact| {
            fact.kind == SemanticFactKind::Unknown
                && fact.certainty == FactCertainty::Unknown
                && fact.target.as_ref().map(SymbolId::as_str) == Some("MissingProjectConfig")
                && fact.origin.method == "cpython_ast"
        }));
    }

    #[test]
    fn product_parser_rejects_unbound_and_shadowed_setup_py_calls() {
        let unbound_sources = [
            r#"def setup(**kwargs):
    return kwargs

def find_packages(*args, **kwargs):
    return []

setup(name="local-project", package_dir={"": "local-src"}, packages=find_packages("local-packages"))
"#,
            r#"import helper

helper.setup(
    name="helper-project",
    package_dir={"": "helper-src"},
    packages=helper.find_packages(where="helper-packages"),
)
"#,
            r#"from setuptools import find_packages, setup

setup = helper.setup
find_packages = helper.find_packages
setup(
    name="shadowed-project",
    package_dir={"": "shadowed-src"},
    packages=find_packages(where="shadowed-packages"),
)
"#,
            r#"from setuptools import setup

if False:
    setup(name="dead-project", package_dir={"": "dead-src"})
"#,
            r#"from setuptools import setup

if flag:
    setup = helper.setup
setup(name="conditional-shadow", package_dir={"": "conditional-src"})
"#,
            r#"from setuptools import setup

del setup
setup(name="deleted-project", package_dir={"": "deleted-src"})
"#,
            r#"from setuptools import find_packages

find_packages(where="standalone-decoy")
"#,
            r#"import setuptools as build_tools

build_tools.setup = helper.setup
build_tools.setup(name="attribute-shadow", package_dir={"": "attribute-src"})
"#,
            r#"import setuptools as build_tools

del build_tools.setup
build_tools.setup(name="attribute-deleted", package_dir={"": "attribute-deleted-src"})
"#,
            r#"import setuptools as build_tools

build_tools.find_packages = helper.find_packages
build_tools.setup(
    name="finder-attribute-shadow",
    packages=build_tools.find_packages(where="finder-attribute-src"),
)
"#,
            r#"import setuptools as build_tools

setattr(build_tools, "setup", helper.setup)
build_tools.setup(name="setattr-shadow", package_dir={"": "setattr-src"})
"#,
            r#"import builtins
import setuptools as build_tools

builtins.setattr(build_tools, "setup", helper.setup)
build_tools.setup(name="builtins-setattr", package_dir={"": "builtins-setattr-src"})
"#,
            r#"import setuptools as build_tools

delattr(build_tools, "find_packages")
build_tools.setup(
    name="delattr-finder",
    packages=build_tools.find_packages(where="delattr-finder-src"),
)
"#,
            r#"import setuptools as build_tools

globals().update({"build_tools": helper})
build_tools.setup(name="globals-update", package_dir={"": "globals-update-src"})
"#,
            r#"import setuptools as build_tools

globals()["build_tools"] = helper
build_tools.setup(name="globals-subscript", package_dir={"": "globals-subscript-src"})
"#,
            r#"import setuptools as build_tools

locals().update({"build_tools": helper})
build_tools.setup(name="locals-update", package_dir={"": "locals-update-src"})
"#,
            r#"import setuptools as build_tools

vars(build_tools)["setup"] = helper.setup
build_tools.setup(name="vars-shadow", package_dir={"": "vars-src"})
"#,
            r#"import builtins
import setuptools as build_tools

builtins.vars(build_tools)["setup"] = helper.setup
build_tools.setup(name="builtins-vars", package_dir={"": "builtins-vars-src"})
"#,
            r#"import setuptools as build_tools

build_tools.__dict__["setup"] = helper.setup
build_tools.setup(name="dict-subscript", package_dir={"": "dict-subscript-src"})
"#,
            r#"import setuptools as build_tools

build_tools.__dict__.update({"find_packages": helper.find_packages})
build_tools.setup(
    name="dict-update-finder",
    packages=build_tools.find_packages(where="dict-update-src"),
)
"#,
        ];

        for source in unbound_sources {
            let report = RepoGrammarSourceParser::default()
                .parse(python_config_document("setup.py", source))
                .expect("unbound setup.py calls remain conservative config results");
            assert_eq!(report.units.len(), 1);
            assert!(report.semantic_facts.is_empty(), "{report:?}");
        }
    }

    #[test]
    fn product_parser_rejects_ambiguous_setup_py_argument_shapes() {
        let ambiguous_sources = [
            r#"from setuptools import setup

setup("positional-name", package_dir={"": "positional-forged"})
"#,
            r#"from setuptools import setup

setup(**dynamic, package_dir={"": "unpack-forged"})
"#,
            r#"from setuptools import setup

setup(package_dir={"": "first-root"}, package_dir={"": "duplicate-root"})
"#,
            r#"from setuptools import setup

setup(name=helper())
"#,
            r#"from setuptools import setup

setup(packages=dynamic)
"#,
            r#"from setuptools import setup

setup(name="dynamic-key", package_dir={helper(): "dynamic-key-root"})
"#,
            r#"from setuptools import setup

setup(name="dict-unpack", package_dir={**mapping, "": "dict-unpack-root"})
"#,
            r#"from setuptools import setup

setup(name="duplicate-key", package_dir={"": "first-root", "": "duplicate-key-root"})
"#,
            r#"from setuptools import setup

setup(name="dynamic-value", package_dir={"": helper()})
"#,
            r#"from setuptools import find_packages, setup

setup(name="positional-where", packages=find_packages("src", where="positional-where-root"))
"#,
            r#"from setuptools import find_packages, setup

setup(name="finder-unpack", packages=find_packages(where="finder-unpack-root", **dynamic))
"#,
            r#"from setuptools import find_packages, setup

setup(name="duplicate-where", packages=find_packages(where="first-root", where="duplicate-where-root"))
"#,
            r#"from setuptools import find_packages, setup

setup(name="dynamic-where", packages=find_packages(where=helper()))
"#,
            r#"from setuptools import setup

setup(name="lookalike-finder", packages=helper.find_packages(where="lookalike-root"))
"#,
            r#"from setuptools import setup

raise RuntimeError("setup is unreachable")
setup(name="dead-config", package_dir={"": "dead-config-root"})
"#,
        ];

        for source in ambiguous_sources {
            let report = RepoGrammarSourceParser::default()
                .parse(python_config_document("setup.py", source))
                .expect("ambiguous setup.py shapes remain typed config results");
            assert!(
                report.semantic_facts.iter().all(|fact| {
                    !fact
                        .assumptions
                        .iter()
                        .any(|assumption| assumption.starts_with("python_config_source_root="))
                }),
                "{report:?}"
            );
            assert!(
                report.semantic_facts.iter().any(|fact| {
                    fact.kind == SemanticFactKind::Unknown
                        && fact.certainty == FactCertainty::Unknown
                        && fact.target.as_ref().map(SymbolId::as_str)
                            == Some("MissingProjectConfig")
                }),
                "{report:?}"
            );
        }
    }

    #[test]
    fn product_parser_accepts_empty_setup_call_without_unknown() {
        let report = RepoGrammarSourceParser::default()
            .parse(python_config_document(
                "setup.py",
                "from setuptools import setup\nsetup()\n",
            ))
            .expect("empty setup.py call is a complete empty static config");

        assert!(report.semantic_facts.is_empty(), "{report:?}");
    }

    #[test]
    fn product_parser_accepts_aliased_setuptools_setup_py_calls() {
        let source = r#"from setuptools import setup as configure
import setuptools as build_tools

configure(
    name="aliased-project",
    package_dir={"": "aliased-src"},
    packages=build_tools.find_namespace_packages(where="aliased-packages"),
)
"#;

        let report = RepoGrammarSourceParser::default()
            .parse(python_config_document("setup.py", source))
            .expect("setuptools aliases remain supported");

        for target in [
            "python.project_config.project_name.aliased-project",
            "python.project_config.source_root.aliased-packages",
            "python.project_config.source_root.aliased-src",
        ] {
            assert!(report
                .semantic_facts
                .iter()
                .any(|fact| { fact.target.as_ref().map(SymbolId::as_str) == Some(target) }));
        }
    }

    #[test]
    fn product_parser_rejects_multiple_authoritative_setup_py_calls_as_conflicting() {
        let source = r#"from setuptools import setup

setup(name="first-project", package_dir={"": "first-src"})
setup(name="second-project", package_dir={"": "second-src"})
"#;

        let report = RepoGrammarSourceParser::default()
            .parse(python_config_document("setup.py", source))
            .expect("multiple setup calls become a typed config conflict");

        assert_eq!(report.units.len(), 1);
        assert_eq!(report.semantic_facts.len(), 2);
        assert!(report.semantic_facts.iter().all(|conflict| {
            conflict.kind == SemanticFactKind::Unknown
                && conflict.certainty == FactCertainty::Unknown
                && conflict.target.as_ref().map(SymbolId::as_str) == Some("ConflictingFacts")
        }));
        for affected_claim in ["python_dependency_inventory", "python_project_config"] {
            assert!(report.semantic_facts.iter().any(|conflict| {
                conflict
                    .assumptions
                    .iter()
                    .any(|assumption| assumption == &format!("affected_claim={affected_claim}"))
            }));
        }
    }

    #[test]
    fn product_parser_scans_setup_py_bindings_in_source_order() {
        let mut source = "setup()\n".repeat(512);
        source.push_str(
            "from setuptools import setup\nsetup(name='linear-project', package_dir={'': 'linear-src'})\n",
        );

        let report = RepoGrammarSourceParser::default()
            .parse(python_config_document("setup.py", &source))
            .expect("pre-import setup candidates remain unbound");

        assert!(report.semantic_facts.iter().any(|fact| {
            fact.target.as_ref().map(SymbolId::as_str)
                == Some("python.project_config.project_name.linear-project")
        }));
        assert!(report.semantic_facts.iter().any(|fact| {
            fact.target.as_ref().map(SymbolId::as_str)
                == Some("python.project_config.source_root.linear-src")
        }));
        assert!(report.semantic_facts.iter().all(|fact| {
            fact.target.as_ref().map(SymbolId::as_str) != Some("ConflictingFacts")
        }));
    }

    #[test]
    fn product_parser_does_not_misroute_similar_paths_as_setup_py_config() {
        for path in ["setup_helper.py", "setup.py.bak", "nested/setup.py"] {
            assert!(matches!(
                RepoGrammarSourceParser::default()
                    .parse(python_config_document(path, "setup(name='not-config')\n")),
                Err(ParseError::UnsupportedLanguage)
            ));
        }
    }
}
