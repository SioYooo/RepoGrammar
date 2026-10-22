//! Temporary ADR-0058 differential: the legacy hand-written parser against the
//! Tree-sitter reading. Deleted with the legacy parser.

use super::{legacy, parse_file, USE_LEGACY};
use crate::adapters::parsing::swift::xctest;
use crate::core::model::{ContentHash, Language, RepositoryRevision};
use crate::ports::parser::SourceDocument;
use std::path::{Path, PathBuf};

fn document<'a>(path: &'a str, text: &'a str) -> SourceDocument<'a> {
    SourceDocument {
        path,
        language: Language::Swift,
        content_hash: ContentHash::new(
            "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        )
        .expect("hash"),
        repository_revision: RepositoryRevision::new("UNKNOWN").expect("revision"),
        text,
    }
}

fn full_output(text: &str, mode: u8) -> String {
    USE_LEGACY.with(|flag| flag.set(mode));
    let output = xctest::parse_output(document("tests/Differential.swift", text));
    USE_LEGACY.with(|flag| flag.set(0));
    format!("{output:?}")
}

fn tree_file(text: &str) -> String {
    format!("{:?}", parse_file(text).expect("grammar parses"))
}

fn legacy_file(text: &str) -> String {
    format!("{:?}", legacy::parse_file_patched(text))
}

/// Compare the parsed structure and the full parse output against the legacy
/// parser with its `init`/`deinit` defect repaired; the defect itself is pinned
/// by its own test.
fn differences(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    let (new, old) = (tree_file(text), legacy_file(text));
    if new != old {
        found.push(format!("structure\n  new: {new}\n  old: {old}"));
    }
    if full_output(text, 0) != full_output(text, 2) {
        found.push("full SourceParseOutput differs".to_string());
    }
    found
}

fn fixture_sources() -> Vec<(PathBuf, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/fixtures/swift");
    let mut files = Vec::new();
    let mut stack = vec![root];
    while let Some(directory) = stack.pop() {
        for entry in std::fs::read_dir(&directory).expect("read fixtures") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                stack.push(path);
            } else if path
                .extension()
                .is_some_and(|extension| extension == "swift")
            {
                let text = std::fs::read_to_string(&path).expect("read fixture");
                files.push((path, text));
            }
        }
    }
    files.sort();
    files
}

fn class(body: &str) -> String {
    format!("import XCTest\nfinal class CatalogTests: XCTestCase {{\n{body}}}\n")
}

/// Sources for which the new reading must equal the legacy one. Inputs where
/// the declared divergence applies (a syntax error inside a region the legacy
/// parser skipped) are deliberately absent.
fn branch_corpus() -> Vec<(String, String)> {
    let mut corpus: Vec<(String, String)> = Vec::new();
    let mut add = |name: &str, text: String| corpus.push((name.to_string(), text));

    // -- lexical refusals -----------------------------------------------------
    add("unterminated_string", class("    let s = \"never closed\n"));
    add("unterminated_comment", class("    /* never closed\n"));
    add("unterminated_backtick", class("    let x = `name\n"));
    add("raw_string", class("    let s = #\"raw\"#\n"));
    add("non_ascii_code", class("    let π = 3.14159\n"));
    add("hash_digit", "import XCTest\nlet x = #1\n".to_string());
    add("regex", class("    let pattern = /a{2,}/\n"));
    add(
        "division",
        class("    func testSplits() {\n        let half = total / 2\n    }\n"),
    );
    add(
        "conditional_file",
        "#if DEBUG\nimport XCTest\n#endif\n".to_string(),
    );
    add(
        "conditional_member",
        class("    #if os(Linux)\n        skip()\n    #endif\n"),
    );
    add("unicode_in_string_comment", class("    func testLocalized() {\n        let greeting = \"目录カタログ\"\n        // コメント with ünïcöde\n    }\n"));

    // -- attribute / generic / accessor / shape / construct / header ----------
    add(
        "attr_arg",
        class("    @available(iOS 15, *)\n    func testNew() {\n    }\n"),
    );
    add(
        "attr_empty_args",
        class("    @discardableResult\n    @objc()\n    func testPlain() {\n    }\n"),
    );
    add(
        "attr_arg_on_class",
        "import XCTest\n@available(macOS 12, *)\nfinal class A: XCTestCase {\n}\n".to_string(),
    );
    add(
        "attr_arg_on_struct",
        "import XCTest\n@available(macOS 12, *)\nstruct S {\n}\n".to_string(),
    );
    add(
        "attr_after_modifier",
        class("    public @objc func testOrder() {\n    }\n"),
    );
    add(
        "generic_method",
        class("    func testGeneric<T>() {\n    }\n"),
    );
    add(
        "generic_class",
        "import XCTest\nclass S<T: XCTestCase> {\n}\n".to_string(),
    );
    add(
        "generic_free_func",
        "import XCTest\nfunc helper<T>(_ v: T) {\n}\n".to_string(),
    );
    add(
        "accessor_computed",
        class("    var count: Int {\n        get { 1 }\n    }\n"),
    );
    add("accessor_shorthand", class("    var count: Int { 1 }\n"));
    add(
        "accessor_closure",
        class("    let onClick = { print(1) }\n"),
    );
    add(
        "accessor_lazy_closure",
        class("    lazy var thing: Int = {\n        1\n    }()\n"),
    );
    add("accessor_didset", class("    var w = 1 { didSet { } }\n"));
    add("accessor_in_parens_ok", class("    let x = foo({ 1 })\n"));
    add(
        "accessor_file_level",
        "import XCTest\nvar g: Int { 1 }\n".to_string(),
    );
    add("plain_properties", class("    static let items: [Int] = []\n    private var total = 0\n    var a = 1; var b = 2\n"));
    add(
        "typealias_member",
        class("    typealias Handler = (Int) -> Void\n    func testOk() { }\n"),
    );
    add(
        "subscript_member",
        class("    subscript(index: Int) -> Int {\n        1\n    }\n"),
    );
    add("nested_struct_member", class("    struct Nested { }\n"));
    add("nested_enum_member", class("    enum Mode { case a }\n"));
    add("associatedtype_member", class("    associatedtype T\n"));
    add(
        "unknown_member_modifier",
        class("    static prefix func - (x: Self) -> Self { x }\n"),
    );
    add("private_set", class("    private(set) var x = 1\n"));
    add(
        "unknown_file_modifier",
        "import XCTest\nindirect enum E { case a }\n".to_string(),
    );
    add(
        "precedence_group",
        "import XCTest\nprecedencegroup TestPrecedence {\n}\n".to_string(),
    );
    add(
        "operator_decl",
        "import XCTest\nprefix operator +++\n".to_string(),
    );
    add("top_level_call", "import XCTest\nXCTMain([])\n".to_string());
    add("file_level_init", "import XCTest\ninit() { }\n".to_string());
    add(
        "file_static_func",
        "import XCTest\nstatic func testX() { }\n".to_string(),
    );
    add("exported_import", "@_exported import XCTest\n".to_string());
    add("public_import", "public import XCTest\n".to_string());
    add(
        "testable_import",
        "@testable import CatalogKit\nimport XCTest\n".to_string(),
    );
    add(
        "preconcurrency_import",
        "@preconcurrency import XCTest\n".to_string(),
    );
    add(
        "import_semicolon",
        "import XCTest; import Foundation\n".to_string(),
    );
    add(
        "class_header_wrapped",
        "import XCTest\nclass CatalogTests:\n    XCTestCase {\n}\n".to_string(),
    );
    add(
        "class_brace_next_line",
        "import XCTest\nclass CatalogTests: XCTestCase\n{\n}\n".to_string(),
    );
    add(
        "class_no_inherit_brace_next_line",
        "import XCTest\nclass Plain\n{\n}\n".to_string(),
    );
    add(
        "func_brace_next_line",
        class("    func testLoads()\n    {\n    }\n"),
    );
    add(
        "func_wrapped_params",
        class("    private func helper(\n        _ value: Int,\n        _ scale: Int) {\n    }\n"),
    );
    add(
        "func_wrapped_return",
        class("    func testLoads()\n        -> Void {\n    }\n"),
    );
    add(
        "func_missing_body",
        class("    func testMissing()\n    func testNext() { }\n"),
    );
    add(
        "typed_throws",
        class("    func testTyped() throws(MyError) { }\n"),
    );
    add(
        "where_clause",
        class("    func testWhere() where Self: Equatable { }\n"),
    );
    add("many_modifiers", class("    public private internal open final static override mutating weak func testMany() { }\n"));
    add(
        "many_attributes",
        class("    @a @b @c @d @e @f @g @h @i func testMany() { }\n"),
    );
    add(
        "long_backtick_name",
        class(&format!("    func `{}`() {{ }}\n", "n".repeat(300))),
    );
    add(
        "long_var_head",
        class(&format!("    var big = [{}]\n", "1, ".repeat(300))),
    );
    add(
        "long_opaque_head",
        format!(
            "import XCTest\nstruct S: {} {{\n}}\n",
            vec!["P"; 300].join(", ")
        ),
    );

    // -- structure ------------------------------------------------------------
    add(
        "unclosed_class",
        "import XCTest\nclass A: XCTestCase {\n".to_string(),
    );
    add(
        "unclosed_class_with_later_generic",
        "import XCTest\nclass A: XCTestCase {\n    func testB<T>() {}\n".to_string(),
    );
    add("stray_closer", "import XCTest\n}\n".to_string());
    add(
        "stray_closer_after_class",
        "import XCTest\nclass A: XCTestCase {\n}\n}\n".to_string(),
    );
    add(
        "extra_closer_in_class",
        "import XCTest\nclass A: XCTestCase {\n    func testA() { }\n}\n    func testB() { }\n}\n"
            .to_string(),
    );
    add(
        "unclosed_method_body",
        class("    func testA() {\n        if x {\n"),
    );
    add("empty", String::new());
    add("whitespace", "\n\n".to_string());
    add("comment_only", "// only a comment\n".to_string());
    add(
        "deep_classes",
        format!(
            "import XCTest\nclass A: XCTestCase {{\n{}{}\n}}\n",
            "    class B {\n".repeat(80),
            "    }\n".repeat(80)
        ),
    );
    add(
        "deep_classes_exact_limit",
        format!(
            "import XCTest\nclass A: XCTestCase {{\n{}{}\n}}\n",
            "    class B {\n".repeat(63),
            "    }\n".repeat(63)
        ),
    );
    add("deep_bodies", format!("import XCTest\nfinal class Deep: XCTestCase {{\n    func testDeep() {{\n{}{}\n    }}\n}}\n", "        if true {\n".repeat(400), "        }\n".repeat(400)));

    // -- anchors and near-misses ----------------------------------------------
    // Compared against the legacy parser with its init/deinit defect repaired.
    add("init_deinit", class("    init?() { super.init() }\n    deinit { }\n    override init() { super.init() }\n    func testA() { }\n"));
    add("init_modifiers", class("    required init(x: Int) { super.init() }\n    convenience init(y: Int) { self.init() }\n    public override init() { super.init() }\n    func testA() { }\n"));
    add(
        "init_wrapped_params",
        class("    init(\n        a: Int,\n        b: Int) {\n    }\n"),
    );
    add("init_brace_next_line", class("    init()\n    {\n    }\n"));
    add("init_generic", class("    init<T>(v: T) { }\n"));
    add(
        "init_attr_arg",
        class("    @available(iOS 15, *)\n    init() { }\n"),
    );
    add(
        "deinit_brace_next_line",
        class("    deinit\n    {\n    }\n"),
    );
    add("macro_attr_arg", "import XCTest\n@freestanding(expression) macro m() = #externalMacro(module: \"M\", type: \"T\")\n".to_string());
    add(
        "class_and_static_vars",
        class("    class var shared = 1\n    static var count = 2\n    func testA() { }\n"),
    );
    add(
        "exact",
        class("    func testA() { }\n    func testB() { }\n    func helper() { }\n"),
    );
    add("overrides", class("    override func setUp() { super.setUp() }\n    override func tearDown() { }\n    func testA() { }\n"));
    add("static_class_methods", class("    static func testStatic() { }\n    class func testClassLevel() { }\n    func testA() { }\n"));
    add("params_returns", class("    func testWithParameter(x: Int) -> Int { x }\n    func testB() -> Void { }\n    func testC() -> Swift.Void { }\n    func testD() -> () { }\n    func testE() async -> Void { }\n    func testF() throws -> Int { 1 }\n    func testG() -> ( ) { }\n"));
    add("effects", class("    func testT() throws { }\n    func testA() async throws { }\n    func testR() rethrows { }\n    func testAA() async { }\n"));
    add("name_near_misses", class("    func test() { }\n    func testx() { }\n    func Test1() { }\n    func test_snake() { }\n    func testing() { }\n    func atest() { }\n"));
    add("backticked_and_operators", class("    static func == (lhs: Self, rhs: Self) -> Bool { true }\n    func < (other: Self) -> Bool { false }\n    func `testBackticked`() { }\n    static func +++ (a: Self, b: Self) -> Self { a }\n"));
    add("nested_class", class("    class Inner: XCTestCase {\n        func testInner() { }\n    }\n    class func testClassLevel() { }\n"));
    add(
        "nested_plain_class",
        class("    class Helper {\n        func testInHelper() { }\n    }\n"),
    );
    add(
        "free_function",
        "import XCTest\nfunc testFreeStanding() {\n    print(1)\n}\n".to_string(),
    );
    add(
        "free_function_non_test",
        "import XCTest\nfunc helper() {\n}\n".to_string(),
    );
    add(
        "free_function_with_params",
        "import XCTest\nfunc testFree(x: Int) {\n}\n".to_string(),
    );
    add("inheritance_variants", "import XCTest\nclass A: XCTestCase {\n}\nclass B: ObservableObject, XCTestCase {\n}\nclass D: Other.XCTestCase, XCTestCaseSubclass {\n}\nclass E: ObservableObject {\n}\nclass F: XCTestCase, Foo<Int> {\n}\nclass G: Foo<XCTestCase> {\n}\n".to_string());
    add(
        "no_import",
        "class A: XCTestCase {\n    func testA() { }\n}\n".to_string(),
    );
    add(
        "foreign_import",
        "import Foundation\nclass A: XCTestCase {\n    func testA() { }\n}\n".to_string(),
    );
    add("import_kinds", "import XCTest\n@testable import CatalogKit\nimport class XCTest.XCTestCase\nimport struct Foundation.Date\nimport func Darwin.sqrt\n".to_string());
    add("opaque_types", "import XCTest\nprivate struct Stub: Identifiable {\n    var value = 1\n}\nextension Stub {\n    func method() { }\n}\nenum E { case a }\nactor Act { func testHidden() { } }\nprotocol P { func a() }\nfinal class CatalogTests: XCTestCase {\n    func testLoadsCatalog() { }\n}\n".to_string());
    add("extension_does_not_reopen", "import XCTest\nfinal class A: XCTestCase {\n    func testA() { }\n}\nextension A {\n    func testB() { }\n}\n".to_string());
    add("wrapper_attributes", "import XCTest\nfinal class A: XCTestCase {\n    @Published var x = 1\n    @State private var y = 2\n    @objc dynamic var z = 3\n    func testA() { }\n}\n".to_string());
    add(
        "main_attribute",
        "import XCTest\n@main\nstruct App {\n}\n".to_string(),
    );
    add("mainactor_class", "import XCTest\n@MainActor\nfinal class A: XCTestCase {\n    @MainActor func testA() { }\n}\n".to_string());
    add("access_modifiers", "import XCTest\npublic final class A: XCTestCase {\n    public func testA() { }\n    internal func testB() { }\n    fileprivate func testC() { }\n    open func testD() { }\n}\n".to_string());
    add("comments_hide_decls", class("    /* outer /* inner */ func testHidden() { } */\n    // func testAlsoHidden() { }\n    func testVisible() { }\n"));
    add("strings_with_braces", class("    func testBuildsMessage() {\n        let message = \"value \\(dictionary[\"key\"] ?? \"fallback {\") end\"\n        let label = \"count \\(items.filter { $0 > 1 }.count)\"\n    }\n"));
    add("multiline_string", class("    func testRendersTemplate() {\n        let template = \"\"\"\n        func fake() { }\n        \"quoted \\(1 + 1) text\"\n        \"\"\"\n    }\n"));
    add(
        "same_line_decls",
        "import XCTest\nclass A: XCTestCase { func testA() { }; func testB() { } }\n".to_string(),
    );
    add(
        "semicolons",
        "import XCTest;\nclass A: XCTestCase {\n    func testA() { };\n    var x = 1;\n}\n"
            .to_string(),
    );
    add("hash_constructs", "import XCTest\n#warning(\"todo\")\n#selector(foo)\nclass A: XCTestCase {\n    func testA() { let s = #selector(foo) }\n}\n".to_string());
    add("file_level_vars", "import XCTest\nlet g = 3\nvar h: Int = 4\ntypealias T = Int\nclass A: XCTestCase {\n    func testA() { }\n}\n".to_string());
    add("closure_heavy_body", class("    func testClosures() {\n        let xs = [1, 2, 3].map { $0 * 2 }.filter { $0 > 2 }\n        let f: (Int) -> Int = { value in\n            switch value {\n            case 1: return 2\n            default: return 3\n            }\n        }\n        _ = f(xs[0])\n    }\n"));
    add("async_await_body", class("    func testAsync() async throws {\n        let value = try await load()\n        XCTAssertEqual(value, 1)\n    }\n"));
    add("generics_in_body", class("    func testGenerics() {\n        let d: Dictionary<String, Array<Int>> = [:]\n        _ = d\n    }\n"));
    add("multiple_classes", "import XCTest\nclass A: XCTestCase {\n    func testA() { }\n}\nclass B: XCTestCase {\n    func testB() { }\n    func testC() { }\n}\n".to_string());
    add(
        "crlf",
        "import XCTest\r\nfinal class A: XCTestCase {\r\n    func testA() { }\r\n}\r\n".to_string(),
    );
    add(
        "no_trailing_newline",
        "import XCTest\nfinal class A: XCTestCase {\n    func testA() { }\n}".to_string(),
    );
    corpus
}

#[test]
fn legacy_and_tree_sitter_agree_on_every_committed_fixture() {
    let fixtures = fixture_sources();
    assert!(!fixtures.is_empty());
    for (path, text) in &fixtures {
        let found = differences(text);
        assert!(found.is_empty(), "{}: {found:?}", path.display());
    }
}

#[test]
fn legacy_and_tree_sitter_agree_on_the_branch_corpus() {
    let mut failures = Vec::new();
    for (name, text) in branch_corpus() {
        let found = differences(&text);
        if !found.is_empty() {
            failures.push(format!("{name}: {found:?}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn the_branch_corpus_reaches_every_refusal_class() {
    use super::Refusal::*;
    let reached: Vec<_> = branch_corpus()
        .iter()
        .filter_map(|(_, text)| legacy::parse_file(text).err())
        .collect();
    for refusal in [
        UnterminatedString,
        UnterminatedBlockComment,
        UnterminatedBacktickName,
        RawStringLiteral,
        NonAsciiCode,
        RegexLiteral,
        ConditionalCompilation,
        AttributeArgument,
        GenericDeclaration,
        UnadmittedAccessor,
        UnadmittedMemberShape,
        UnadmittedFileConstruct,
        UnadmittedDeclarationHeader,
        UnbalancedBlockStructure,
        UnadmittedHashConstruct,
        ResourceLimit,
    ] {
        assert!(reached.contains(&refusal), "{refusal:?} is not reached");
    }
}

/// Declared divergence `syntax_error_in_skipped_region`: the legacy parser
/// consumed a body by brace depth and never read its statements, while the
/// grammar refuses the file on a syntax error anywhere. The new behaviour is
/// pinned here instead of compared.
#[test]
fn a_syntax_error_in_a_skipped_body_refuses_the_file_where_the_legacy_parser_admitted_it() {
    let text = class("    func testA() {\n        let = = 1 +\n    }\n");
    assert!(legacy::parse_file(&text).is_ok());
    assert_eq!(
        parse_file(&text).expect("grammar parses").unwrap_err(),
        super::Refusal::UnadmittedFileConstruct
    );
}

/// tree-sitter-swift 0.7.4 cannot read some valid Swift that the legacy parser
/// skipped by brace depth; the file then refuses as an unadmitted construct.
/// Pinned so a grammar bump that fixes them is noticed.
#[test]
fn valid_swift_the_pinned_grammar_cannot_read_refuses_the_file() {
    for body in [
        "    func testA() {\n        nonisolated(unsafe) let data = 1\n    }\n",
        "    func testA() { emit(value: ()) }\n",
        "    func testA() {\n        Button { action() }\n        label: {\n            Text(\"x\")\n        }\n    }\n",
        "    func testA() { let t: [Any] = [~String.self] }\n",
    ] {
        let text = class(body);
        assert!(legacy::parse_file_patched(&text).is_ok(), "{body:?}");
        assert!(parse_file(&text).expect("grammar parses").is_err(), "{body:?}");
    }
}

/// The legacy parser read `init` and `deinit` heads as if the keyword were
/// followed by a function name, so it refused every class that declared one.
/// The grammar admits them; they never anchor.
#[test]
fn initializers_are_admitted_where_the_legacy_parser_refused_them() {
    let text = class(
        "    init?() { super.init() }\n    deinit { }\n    override init() { super.init() }\n    required init(x: Int) { super.init() }\n    func testA() { }\n",
    );
    assert!(legacy::parse_file(&text).is_err());
    let parsed = parse_file(&text)
        .expect("grammar parses")
        .expect("initializers are admitted");
    assert_eq!(parsed.classes[0].methods.len(), 1);
    assert_eq!(
        parse_file(&class("    init<T>(v: T) { }\n"))
            .expect("grammar parses")
            .unwrap_err(),
        super::Refusal::GenericDeclaration
    );
}

/// `REPOGRAMMAR_SWIFT_DIFFERENTIAL_CORPUS=<dir>` compares the two parsers over
/// every `.swift` file below `<dir>` and prints a per-file summary without
/// failing on a difference.
#[test]
#[ignore]
fn external_corpus_differential() {
    let Ok(root) = std::env::var("REPOGRAMMAR_SWIFT_DIFFERENTIAL_CORPUS") else {
        return;
    };
    let mut files = Vec::new();
    let mut stack = vec![PathBuf::from(root)];
    while let Some(directory) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path
                .extension()
                .is_some_and(|extension| extension == "swift" || extension == "template")
            {
                files.push(path);
            }
        }
    }
    files.sort();
    let (mut compared, mut differing, mut differing_patched, mut panics) = (0usize, 0, 0, 0);
    for path in files {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        compared += 1;
        let summarize = |result: Result<super::SwiftFile, super::Refusal>| match result {
            Ok(file) => format!("{file:?}"),
            Err(refusal) => format!("{refusal:?}"),
        };
        let old = std::panic::catch_unwind(|| summarize(legacy::parse_file(&text)));
        let patched = std::panic::catch_unwind(|| summarize(legacy::parse_file_patched(&text)));
        let new =
            std::panic::catch_unwind(|| summarize(parse_file(&text).expect("grammar parses")));
        let (Ok(old), Ok(patched), Ok(new)) = (old, patched, new) else {
            panics += 1;
            println!("PANIC {}", path.display());
            continue;
        };
        let label = |value: &str| {
            if value.starts_with("SwiftFile") {
                "admitted".to_string()
            } else {
                value.to_string()
            }
        };
        if old != new {
            differing += 1;
            println!(
                "DIFF {} legacy={} new={}{}",
                path.display(),
                label(&old),
                label(&new),
                if old.starts_with("SwiftFile") && new.starts_with("SwiftFile") {
                    " (structure)"
                } else {
                    ""
                }
            );
        }
        if patched != new {
            differing_patched += 1;
            println!(
                "DIFFP {} patched={} new={}{}",
                path.display(),
                label(&patched),
                label(&new),
                if patched.starts_with("SwiftFile") && new.starts_with("SwiftFile") {
                    " (structure)"
                } else {
                    ""
                }
            );
        }
        println!(
            "STAT legacy={} patched={} new={}",
            label(&old),
            label(&patched),
            label(&new)
        );
    }
    println!(
        "SUMMARY compared={compared} differing={differing} differing_vs_patched={differing_patched} panics={panics}"
    );
}
