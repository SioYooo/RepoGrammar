//! Parsing adapters. Tree-sitter types must not cross this module boundary.

use crate::core::model::{CodeUnit, CodeUnitKind, IrEdge, IrEdgeLabel, IrNode, IrNodeId};
use crate::ports::parser::{
    ParseError, ParseReport, ParserProjectContext, PythonInterfaceProbe, SourceDocument,
    SourceParseOutput, SourceParser,
};
use std::collections::BTreeSet;

pub mod cpp;
pub mod csharp;
pub mod java;
pub mod python;
pub mod rust;
pub mod syntax;
pub mod tree_sitter;
pub mod tsjs;

#[derive(Debug, Default)]
pub struct RepoGrammarSourceParser {
    syntax: syntax::SyntaxCodeUnitParser,
    python: python::PythonAstParser,
    java: java::JavaSyntaxParser,
    csharp: csharp::CSharpSyntaxParser,
    cpp: cpp::CppSyntaxParser,
    rust: rust::RustSyntaxParser,
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
            crate::core::model::Language::CSharp => self.csharp.parse(document),
            crate::core::model::Language::C
            | crate::core::model::Language::Cpp
            | crate::core::model::Language::CppConfig => self.cpp.parse(document),
            crate::core::model::Language::Go
            | crate::core::model::Language::GoConfig
            | crate::core::model::Language::Php
            | crate::core::model::Language::PhpConfig
            | crate::core::model::Language::Ruby
            | crate::core::model::Language::RubyConfig
            | crate::core::model::Language::Swift
            | crate::core::model::Language::SwiftConfig => Err(ParseError::UnsupportedLanguage),
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
            crate::core::model::Language::CSharp => {
                self.csharp.parse_with_context(document, context)
            }
            crate::core::model::Language::C
            | crate::core::model::Language::Cpp
            | crate::core::model::Language::CppConfig => {
                self.cpp.parse_with_context(document, context)
            }
            crate::core::model::Language::Go
            | crate::core::model::Language::GoConfig
            | crate::core::model::Language::Php
            | crate::core::model::Language::PhpConfig
            | crate::core::model::Language::Ruby
            | crate::core::model::Language::RubyConfig
            | crate::core::model::Language::Swift
            | crate::core::model::Language::SwiftConfig => Err(ParseError::UnsupportedLanguage),
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
            crate::core::model::Language::Python | crate::core::model::Language::PythonConfig => {
                self.python.parse_with_context_output(document, context)
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
            | "aspnet_controller"
            | "efcore_db_context"
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
        SourceDocument {
            path: match language {
                Language::Ruby => "main.rb",
                Language::RubyConfig => "Gemfile",
                _ => unreachable!("Ruby inventory helper accepts only Ruby tokens"),
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

    #[test]
    fn product_parser_explicitly_rejects_go_inventory_tokens() {
        let parser = RepoGrammarSourceParser::default();
        assert_eq!(
            parser.parse(go_inventory_document(Language::Go)),
            Err(ParseError::UnsupportedLanguage)
        );
        assert_eq!(
            parser.parse_with_context(
                go_inventory_document(Language::GoConfig),
                &ParserProjectContext::default(),
            ),
            Err(ParseError::UnsupportedLanguage)
        );
    }

    #[test]
    fn product_parser_explicitly_rejects_ruby_inventory_tokens() {
        let parser = RepoGrammarSourceParser::default();
        for language in [Language::Ruby, Language::RubyConfig] {
            assert_eq!(
                parser.parse(ruby_inventory_document(language.clone())),
                Err(ParseError::UnsupportedLanguage)
            );
            assert_eq!(
                parser.parse_with_context(
                    ruby_inventory_document(language),
                    &ParserProjectContext::default(),
                ),
                Err(ParseError::UnsupportedLanguage)
            );
        }
    }

    #[test]
    fn product_parser_explicitly_rejects_php_inventory_tokens() {
        let parser = RepoGrammarSourceParser::default();
        for language in [Language::Php, Language::PhpConfig] {
            assert_eq!(
                parser.parse(php_inventory_document(language.clone())),
                Err(ParseError::UnsupportedLanguage)
            );
            assert_eq!(
                parser.parse_with_context(
                    php_inventory_document(language),
                    &ParserProjectContext::default(),
                ),
                Err(ParseError::UnsupportedLanguage)
            );
        }
    }

    #[test]
    fn product_parser_explicitly_rejects_swift_inventory_tokens() {
        let parser = RepoGrammarSourceParser::default();
        for language in [Language::Swift, Language::SwiftConfig] {
            assert_eq!(
                parser.parse(swift_inventory_document(language.clone())),
                Err(ParseError::UnsupportedLanguage)
            );
            assert_eq!(
                parser.parse_with_context(
                    swift_inventory_document(language),
                    &ParserProjectContext::default(),
                ),
                Err(ParseError::UnsupportedLanguage)
            );
        }
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
        assert_eq!(report.semantic_facts.len(), 1);
        let conflict = &report.semantic_facts[0];
        assert_eq!(conflict.kind, SemanticFactKind::Unknown);
        assert_eq!(conflict.certainty, FactCertainty::Unknown);
        assert_eq!(
            conflict.target.as_ref().map(SymbolId::as_str),
            Some("ConflictingFacts")
        );
        assert!(conflict
            .assumptions
            .iter()
            .any(|assumption| assumption == "affected_claim=python_project_config"));
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
