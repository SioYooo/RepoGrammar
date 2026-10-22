//! Temporary ADR-0058 differential: the retired hand-written parser against the
//! Tree-sitter reader, over the committed fixtures, a branch corpus of every
//! refusal class and anchor variation, and an optional external directory.
//!
//! The coordinator deletes this module together with `legacy_syntax` once the
//! fixture-wide capture confirms the lane.

use super::aunit::parse_output_using;
use super::legacy_syntax;
use super::lexer::Refusal;
use super::syntax::{self, AdmittedUnit, MAX_DEPTH};
use crate::core::model::{ContentHash, Language, RepositoryRevision};
use crate::ports::parser::{ParseError, SourceDocument, SourceParseOutput};

fn legacy_parse(source: &str) -> Result<Result<AdmittedUnit, Refusal>, ParseError> {
    Ok(legacy_syntax::parse_compilation(source))
}

fn output_with(
    source: &str,
    parse: fn(&str) -> Result<Result<AdmittedUnit, Refusal>, ParseError>,
) -> SourceParseOutput {
    parse_output_using(
        SourceDocument {
            path: "tests/catalog_tests.adb",
            language: Language::Ada,
            content_hash: ContentHash::new(
                "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            )
            .expect("hash"),
            repository_revision: RepositoryRevision::new("UNKNOWN").expect("revision"),
            text: source,
        },
        parse,
    )
    .expect("parse output")
}

/// Compares the syntax-layer result and the full downstream output.
fn difference(source: &str) -> Option<String> {
    let old = legacy_syntax::parse_compilation(source);
    let new = syntax::parse_compilation(source).expect("grammar loads");
    let old_output = output_with(source, legacy_parse);
    let new_output = output_with(source, syntax::parse_compilation);
    if old != new {
        return Some(format!("syntax: legacy {old:?} vs tree-sitter {new:?}"));
    }
    if old_output != new_output {
        return Some("output differs despite equal syntax results".to_string());
    }
    None
}

const HEAD: &str = "with AUnit.Test_Cases;\npackage body Catalog_Tests is\n\
                    procedure Register_Tests (T : in out Test_Case) is\nbegin\n";
const TAIL: &str = "end Register_Tests;\nend Catalog_Tests;\n";
const REG: &str = "Register_Routine (T, Loads'Access, \"loads\");\n";

fn body(statements: &str) -> String {
    format!("{HEAD}{statements}{TAIL}")
}

/// A declaration inside the package body, before the registering procedure.
fn decl(declaration: &str) -> String {
    format!(
        "with AUnit.Test_Cases;\npackage body Catalog_Tests is\n{declaration}\n\
         procedure Register_Tests (T : in out Test_Case) is\nbegin\n{REG}\
         end Register_Tests;\nend Catalog_Tests;\n"
    )
}

/// A declaration inside the registering procedure, so it is nested one level.
fn local_decl(declaration: &str) -> String {
    format!(
        "with AUnit.Test_Cases;\npackage body Catalog_Tests is\n\
         procedure Register_Tests (T : in out Test_Case) is\n{declaration}\nbegin\n{REG}\
         end Register_Tests;\nend Catalog_Tests;\n"
    )
}

fn package_with(items: &str) -> String {
    format!("package body P is\n{items}end P;\n")
}

fn nested(open: &str, close: &str, levels: usize) -> String {
    format!("{}{}", open.repeat(levels), close.repeat(levels))
}

fn branch_corpus() -> Vec<(String, String)> {
    let mut corpus: Vec<(String, String)> = Vec::new();
    let mut add = |name: &str, source: String| corpus.push((name.to_string(), source));

    // ---- lexical gate refusals -------------------------------------------
    add(
        "preprocessor directive",
        body("#if DEBUG\nnull;\n#end if;\n"),
    );
    add("preprocessor substitution", body("X := $Value;\n"));
    add("replacement percent", body("X := %text%;\n"));
    add(
        "replacement bang",
        body("case X is when 1 ! 2 => null; end case;\n"),
    );
    add("replacement colon", body("X := 16:FF:;\n"));
    add("unterminated string", body("X := \"open;\n"));
    add(
        "unterminated string at eof",
        "with AUnit.Test_Cases;\n\"open".to_string(),
    );
    add("unterminated char", body("X := 'a;\n"));
    add("unterminated based literal", body("X := 16#FF;\n"));
    for (index, character) in ["@", "[", "{", "^", "\u{dc}", "~", "?", "\\", "`"]
        .iter()
        .enumerate()
    {
        add(
            &format!("unsupported character {index}"),
            body(&format!("X := 1 {character} 2;\n")),
        );
    }
    add("midline hash", body("X := 1 # 2;\n"));
    add(
        "directive inside comment is prose",
        body("-- #if $x %y% [z]\nnull;\n"),
    );
    add(
        "directive text inside string is prose",
        body("Note := \"#if $x\";\nnull;\n"),
    );

    // ---- edition-sensitive words -----------------------------------------
    for word in [
        "Interface",
        "Overriding",
        "Parallel",
        "Some",
        "Synchronized",
    ] {
        add(
            &format!("edition word as object {word}"),
            decl(&format!("{word} : Boolean := False;")),
        );
        add(
            &format!("edition word upper {word}"),
            decl(&format!(
                "{} : Boolean := False;",
                word.to_ascii_uppercase()
            )),
        );
    }
    add(
        "overriding procedure",
        decl("overriding procedure Set_Up (T : in out Test_Case) is begin null; end Set_Up;"),
    );
    add(
        "overriding function",
        decl("overriding function Name (T : Test_Case) return String is begin return \"x\"; end Name;"),
    );
    add(
        "not overriding procedure",
        decl("not overriding procedure Set_Up (T : in out Test_Case) is begin null; end Set_Up;"),
    );
    add(
        "overriding not before subprogram",
        decl("overriding X : Integer;"),
    );
    add(
        "overriding upper before procedure",
        decl("OVERRIDING PROCEDURE P is begin null; end P;"),
    );
    add(
        "overriding declaration without body",
        decl("overriding procedure P (T : Test_Case);"),
    );
    add("type with interface", decl("type I is interface;"));
    add(
        "some quantifier",
        body("if (for some I in 1 .. 3 => I = 2) then null; end if;\n"),
    );
    add(
        "all quantifier",
        body("if (for all I in 1 .. 3 => I > 0) then null; end if;\n"),
    );
    add(
        "overriding before identifier",
        decl("Overriding_Count : Natural := 0;"),
    );

    // ---- edition pragmas -------------------------------------------------
    let anchor = body(REG);
    for pragma in [
        "pragma Ada_83;",
        "pragma Ada_1983;",
        "pragma ADA_83;",
        "pragma Extensions_Allowed (On);",
        "pragma Extensions_Allowed;",
        "pragma Ada_95;",
        "pragma Ada_05;",
        "pragma Ada_2005;",
        "pragma Ada_12;",
        "pragma Ada_2012;",
        "pragma Ada_2022;",
        "pragma Pure;",
        "pragma Elaborate_Body;",
        "pragma Restrictions (No_Tasking);",
        "pragma Style_Checks (Off);",
    ] {
        add(
            &format!("leading pragma {pragma}"),
            format!("{pragma}\n{anchor}"),
        );
        add(
            &format!("pragma between with clauses {pragma}"),
            format!("with Ada.Text_IO;\n{pragma}\n{anchor}"),
        );
        add(&format!("package pragma {pragma}"), decl(pragma));
        add(
            &format!("statement pragma {pragma}"),
            body(&format!("{pragma}\n{REG}")),
        );
        add(&format!("local pragma {pragma}"), local_decl(pragma));
        add(
            &format!("trailing pragma {pragma}"),
            format!("{anchor}{pragma}\n"),
        );
    }
    add(
        "pragma before a refused construct",
        format!("pragma Ada_83;\n{}", decl("task T;")),
    );
    add(
        "refused construct before pragma",
        decl("task T;\npragma Ada_83;"),
    );
    add(
        "pragma inside refused task body is still earlier",
        decl("pragma Ada_83;\ntask body T is begin null; end T;"),
    );
    add(
        "unterminated lexical before pragma",
        format!("pragma Ada_83;\n{}", body("X := \"open;\n")),
    );
    add(
        "edition word before pragma",
        format!("pragma Ada_83;\n{}", decl("Interface : Boolean;")),
    );
    add(
        "pragma then syntax error later",
        format!("pragma Ada_83;\n{}", body("X := ;\n")),
    );
    add(
        "syntax error then pragma later",
        body("X := ;\npragma Ada_83;\nnull;\n"),
    );

    // ---- outside the declared subset -------------------------------------
    for (name, declaration) in [
        ("task body", "task body Worker is begin null; end Worker;"),
        ("task type", "task type Worker;"),
        ("single task", "task Worker;"),
        (
            "task type with entry",
            "task type Worker is entry Go; end Worker;",
        ),
        ("protected body", "protected body Guard is end Guard;"),
        (
            "protected type",
            "protected type Guard is procedure P; end Guard;",
        ),
        (
            "single protected",
            "protected Guard is procedure P; end Guard;",
        ),
        (
            "generic procedure",
            "generic\n type Element is private;\n procedure Swap (A : in out Element);",
        ),
        (
            "generic package",
            "generic\n type Element is private;\n package Box is end Box;",
        ),
        (
            "generic function",
            "generic\n type Element is private;\n function Id (A : Element) return Element;",
        ),
        (
            "address clause",
            "for Flags'Address use System.Null_Address;",
        ),
        ("size clause", "for Small'Size use 8;"),
        ("at clause", "for Flags use at 16#1000#;"),
        (
            "record representation",
            "for R use record X at 0 range 0 .. 7; end record;",
        ),
        ("enumeration representation", "for E use (A => 1, B => 2);"),
        ("entry", "entry Go;"),
        ("subunit stub procedure", "procedure Helper is separate;"),
        ("subunit stub package", "package Helper is separate;"),
        (
            "subunit stub function",
            "function Helper return Integer is separate;",
        ),
        ("task stub", "task body Worker is separate;"),
        ("protected stub", "protected body Guard is separate;"),
        ("interface type", "type I is interface;"),
        (
            "synchronized interface",
            "type I is synchronized interface;",
        ),
        ("task interface", "type I is task interface;"),
        ("limited interface", "type I is limited interface;"),
    ] {
        add(&format!("outside: {name}"), decl(declaration));
        add(&format!("outside local: {name}"), local_decl(declaration));
    }
    for (name, statement) in [
        ("accept", "accept Ready;"),
        ("accept with body", "accept Ready do null; end Ready;"),
        (
            "select else",
            "select\n   accept Ready;\nelse\n   null;\nend select;",
        ),
        (
            "timed select",
            "select\n   delay 1.0;\nthen abort\n   null;\nend select;",
        ),
        (
            "select or",
            "select\n   accept A;\nor\n   accept B;\nend select;",
        ),
        ("delay", "delay 1.0;"),
        ("delay until", "delay until T;"),
        ("abort", "abort Worker;"),
        ("requeue", "requeue Ready;"),
        ("terminate", "terminate;"),
        (
            "entry call select",
            "select\n   Worker.Go;\nor\n   delay 1.0;\nend select;",
        ),
    ] {
        add(
            &format!("outside stmt: {name}"),
            body(&format!("{statement}\n")),
        );
    }
    add(
        "subunit",
        "with AUnit.Test_Cases;\nseparate (Catalog_Tests)\nprocedure Register_Tests (T : in out Test_Case) is\nbegin\n   Register_Routine (T, Loads'Access, \"loads\");\nend Register_Tests;\n"
            .to_string(),
    );
    add(
        "package spec",
        "with AUnit.Test_Cases;\npackage Catalog_Tests is\nend Catalog_Tests;\n".to_string(),
    );
    add(
        "package spec with private part",
        "package Catalog_Tests is\nprivate\nX : Integer;\nend Catalog_Tests;\n".to_string(),
    );
    add(
        "library subprogram spec",
        "with AUnit.Test_Cases;\nprocedure Register_Tests (T : in out Test_Case);\n".to_string(),
    );
    add(
        "library function spec",
        "function F return Integer;\n".to_string(),
    );
    add(
        "library generic instantiation",
        "procedure P is new Generic_P (Integer);\n".to_string(),
    );
    add(
        "library package instantiation",
        "package P is new Q (Integer);\n".to_string(),
    );
    add(
        "library package renaming",
        "package P renames Q;\n".to_string(),
    );
    add("empty file", String::new());
    add("only comments", "-- nothing here\n".to_string());
    add("only with clause", "with AUnit.Test_Cases;\n".to_string());
    add("only pragma", "pragma Pure;\n".to_string());
    add("whitespace file", "  \n\n \t\n".to_string());
    add("two units", format!("{anchor}{anchor}"));
    add("body then with", format!("{anchor}with Ada.Text_IO;\n"));
    add("trailing statement", format!("{anchor}null;\n"));
    add("trailing semicolon", format!("{anchor};\n"));
    add("trailing identifier", format!("{anchor}Foo\n"));
    add(
        "private package body",
        "private package body P is end P;\n".to_string(),
    );
    add(
        "private with",
        "private with Ada.Text_IO;\npackage body P is\nend P;\n".to_string(),
    );
    add(
        "limited with",
        "limited with Ada.Text_IO;\npackage body P is\nend P;\n".to_string(),
    );
    add(
        "limited private with",
        "limited private with Ada.Text_IO;\npackage body P is\nend P;\n".to_string(),
    );
    add(
        "use clauses",
        "with Ada.Text_IO;\nuse Ada.Text_IO;\nuse type Ada.Calendar.Time;\nuse all type Ada.Calendar.Time;\npackage body P is\nend P;\n"
            .to_string(),
    );
    add(
        "with list",
        "with Ada.Text_IO, AUnit.Test_Cases, Ada.Strings;\npackage body P is\nend P;\n".to_string(),
    );
    add(
        "with list aunit second",
        "with Ada.Text_IO, AUnit;\npackage body P is\nend P;\n".to_string(),
    );
    add(
        "with aunit child only",
        "with AUnit.Assertions;\npackage body P is\nend P;\n".to_string(),
    );
    add(
        "with lowercase aunit",
        "with aunit.test_cases;\npackage body P is\nend P;\n".to_string(),
    );
    add(
        "with look-alike",
        "with AUnits.Test_Cases;\npackage body P is\nend P;\n".to_string(),
    );
    add(
        "with inside a record is not a clause",
        "with Ada.Text_IO;\npackage body P is\ntype T is new Base with null record;\nend P;\n"
            .to_string(),
    );
    add(
        "aspect with is not a clause",
        "with Ada.Text_IO;\npackage body P is\nprocedure Q with Inline is begin null; end Q;\nend P;\n"
            .to_string(),
    );
    add(
        "package body only",
        "package body P is\nend P;\n".to_string(),
    );
    add(
        "package body no end name",
        "package body P is\nend;\n".to_string(),
    );
    add(
        "package body wrong end name",
        "package body P is\nend Q;\n".to_string(),
    );
    add(
        "package body with begin",
        "package body P is\nbegin\n   null;\nend P;\n".to_string(),
    );
    add(
        "package body empty begin",
        "package body P is\nbegin\nend P;\n".to_string(),
    );
    add(
        "child package body",
        "package body Parent.Child is\nend Parent.Child;\n".to_string(),
    );
    add(
        "package body aspect",
        "package body P with SPARK_Mode is\nend P;\n".to_string(),
    );
    add(
        "package body missing end",
        "package body P is\nprocedure Q is begin null; end Q;\n".to_string(),
    );
    add(
        "library procedure body",
        "with AUnit.Test_Cases;\nprocedure Run is\nbegin\n   Register_Routine (T, Loads'Access, \"loads\");\nend Run;\n"
            .to_string(),
    );
    add(
        "library function body",
        "function F return Integer is\nbegin\n   return 1;\nend F;\n".to_string(),
    );
    add(
        "library operator body",
        "function \"+\" (L, R : T) return T is\nbegin\n   return L;\nend \"+\";\n".to_string(),
    );
    add(
        "library not overriding",
        "not overriding procedure P is\nbegin\n   null;\nend P;\n".to_string(),
    );
    add(
        "library expression function",
        "function F return Integer is (1);\n".to_string(),
    );
    add(
        "library null procedure",
        "procedure P is null;\n".to_string(),
    );
    add("library renaming", "procedure P renames Q;\n".to_string());
    add(
        "library separate stub",
        "procedure P is separate;\n".to_string(),
    );
    add("library abstract", "procedure P is abstract;\n".to_string());

    // ---- statement and syntax-error refusals ------------------------------
    add("empty statement sequence", body(""));
    add(
        "missing semicolon",
        body("Register_Routine (T, Loads'Access, \"loads\")\n"),
    );
    add("missing then", body("if A null; end if;\n"));
    add("unclosed if", body("if A then null;\n"));
    add("unclosed paren", body("Foo (1, 2;\n"));
    add("extra paren", body("Foo (1));\n"));
    add("assignment missing rhs", body("X := ;\n"));
    add("garbage tokens", body("<> <> <>;\n"));
    add("stray end", body("end;\n"));
    add("stray else", body("else null;\n"));
    add("stray when", body("when others => null;\n"));
    add("stray is", body("is null;\n"));
    add("stray tick", body("X := 'Access;\n"));
    add("number then identifier", body("X := 1 Y;\n"));
    add("empty parens", body("Foo ();\n"));
    add("missing loop end", body("loop null; end;\n"));
    add("case without alternatives", body("case X is end case;\n"));
    add("mismatched end", body("if A then null; end loop;\n"));
    add("keyword as name", body("begin := 1;\n"));
    add("return in procedure body", body("return;\n"));
    add("exit outside loop", body("exit;\n"));
    add("goto", body("goto Done;\n<<Done>>\nnull;\n"));
    add("label without statement", body("<<Done>>\n"));

    // ---- anchor variations and near misses --------------------------------
    for (name, call) in [
        ("bare", "Register_Routine (T, Loads'Access, \"loads\");"),
        (
            "bare no space",
            "Register_Routine(T,Loads'Access,\"loads\");",
        ),
        ("upper", "REGISTER_ROUTINE (T, LOADS'ACCESS, \"loads\");"),
        ("lower", "register_routine (t, loads'access, \"loads\");"),
        (
            "prefixed",
            "Registration.Register_Routine (T, Loads'Access, \"loads\");",
        ),
        (
            "long prefixed",
            "AUnit.Test_Cases.Registration.Register_Routine (T, Loads'Access, \"loads\");",
        ),
        (
            "multi line",
            "Register_Routine\n   (T,\n    Loads'Access,\n    \"loads\");",
        ),
        (
            "comment between args",
            "Register_Routine (T, -- the fixture\n Loads'Access, -- routine\n \"loads\");",
        ),
        (
            "dotted routine",
            "Register_Routine (T, Pkg.Sub.Loads'Access, \"loads\");",
        ),
        (
            "string with doubled quote",
            "Register_Routine (T, Loads'Access, \"a \"\"quoted\"\" word\");",
        ),
        ("empty string", "Register_Routine (T, Loads'Access, \"\");"),
        (
            "nested call first",
            "Register_Routine (Fixture (T, 1), Loads'Access, \"loads\");",
        ),
        (
            "nested aggregate first",
            "Register_Routine ((1, 2), Loads'Access, \"loads\");",
        ),
        (
            "qualified first",
            "Register_Routine (T_Ptr'(null), Loads'Access, \"loads\");",
        ),
        (
            "operator symbol name second",
            "Register_Routine (T, \"+\"'Access, \"loads\");",
        ),
        (
            "qualified expr second",
            "Register_Routine (T, Integer'(1), \"loads\");",
        ),
        ("too few", "Register_Routine (T, Loads'Access);"),
        (
            "too many",
            "Register_Routine (T, Loads'Access, \"loads\", Extra);",
        ),
        ("one", "Register_Routine (T);"),
        ("none", "Register_Routine;"),
        (
            "pointer variable",
            "Register_Routine (T, Routine_Ptr, \"loads\");",
        ),
        (
            "description variable",
            "Register_Routine (T, Loads'Access, Description);",
        ),
        (
            "wrong attribute",
            "Register_Routine (T, Loads'Address, \"loads\");",
        ),
        (
            "unrestricted access",
            "Register_Routine (T, Loads'Unrestricted_Access, \"loads\");",
        ),
        (
            "concatenated",
            "Register_Routine (T, Loads'Access, \"a\" & \"b\");",
        ),
        (
            "call result access",
            "Register_Routine (T, Table (1)'Access, \"loads\");",
        ),
        (
            "all access",
            "Register_Routine (T, Ptr.all'Access, \"loads\");",
        ),
        (
            "parenthesised access",
            "Register_Routine (T, (Loads'Access), \"loads\");",
        ),
        (
            "parenthesised string",
            "Register_Routine (T, Loads'Access, (\"loads\"));",
        ),
        (
            "named routine",
            "Register_Routine (T, Routine => Loads'Access, Name => \"loads\");",
        ),
        (
            "named first",
            "Register_Routine (Test => T, Routine => Loads'Access, Name => \"loads\");",
        ),
        (
            "named description only",
            "Register_Routine (T, Loads'Access, Name => \"loads\");",
        ),
        (
            "look-alike name",
            "My_Register_Routine (T, Loads'Access, \"loads\");",
        ),
        (
            "look-alike suffix",
            "Register_Routine_Twice (T, Loads'Access, \"loads\");",
        ),
        (
            "assignment",
            "Handle := Register_Routine (T, Loads'Access, \"loads\");",
        ),
        (
            "nested in call",
            "Report (Register_Routine (T, Loads'Access, \"loads\"));",
        ),
        (
            "access of access",
            "Register_Routine (T, Loads'Access'Access, \"loads\");",
        ),
        (
            "negated string",
            "Register_Routine (T, Loads'Access, - \"loads\");",
        ),
        (
            "access comparison",
            "Register_Routine (T, Loads'Access = Other'Access, \"loads\");",
        ),
        (
            "string comparison",
            "Register_Routine (T, Loads'Access, \"a\" = \"b\");",
        ),
        (
            "character third",
            "Register_Routine (T, Loads'Access, 'x');",
        ),
        ("number third", "Register_Routine (T, Loads'Access, 5);"),
        ("range second", "Register_Routine (T, 1 .. 2, \"loads\");"),
        (
            "choice second",
            "Register_Routine (T, 1 | 2 => X, \"loads\");",
        ),
        (
            "others",
            "Register_Routine (T, others => Loads'Access, \"loads\");",
        ),
        ("box", "Register_Routine (T, <>, \"loads\");"),
        (
            "prefix call result",
            "Fixture (1).Register_Routine (T, Loads'Access, \"loads\");",
        ),
        (
            "extra call suffix",
            "Register_Routine (A) (T, Loads'Access, \"loads\");",
        ),
        (
            "selected after call",
            "Register_Routine (T, Loads'Access, \"loads\").Done;",
        ),
        (
            "attribute after call",
            "Register_Routine (T, Loads'Access, \"loads\")'Size;",
        ),
        (
            "registration with exception",
            "Register_Routine (T, Loads'Access, \"loads\") with Inline;",
        ),
    ] {
        add(&format!("anchor: {name}"), body(&format!("{call}\n")));
        add(
            &format!("anchor no with clause: {name}"),
            format!(
                "package body Catalog_Tests is\nprocedure Register_Tests (T : in out Test_Case) is\nbegin\n{call}\nend Register_Tests;\nend Catalog_Tests;\n"
            ),
        );
    }
    add(
        "two registrations and a non anchor",
        body(&format!(
            "{REG}{REG}Register_Routine (T, Ptr, \"x\");\n{REG}"
        )),
    );
    add(
        "registration in if branches",
        body(&format!(
            "if A then\n{REG}elsif B then\n{REG}else\n{REG}end if;\n"
        )),
    );
    add(
        "registration in loops",
        body(&format!(
            "for I in 1 .. 3 loop\n{REG}end loop;\nwhile A loop\n{REG}end loop;\nloop\n{REG}exit;\nend loop;\n"
        )),
    );
    add(
        "registration in case",
        body(&format!(
            "case X is\nwhen 1 => {REG}when 2 | 3 => {REG}when others => {REG}end case;\n"
        )),
    );
    add(
        "registration in block",
        body(&format!("declare\nX : Integer := 1;\nbegin\n{REG}end;\n")),
    );
    add(
        "registration in handler",
        body(&format!(
            "null;\nexception\nwhen Constraint_Error =>\n{REG}when E : others =>\n{REG}"
        )),
    );
    add(
        "registration in nested procedure",
        decl(&format!("procedure Inner is\nbegin\n{REG}end Inner;")),
    );
    add(
        "registration in package body statements",
        format!("with AUnit.Test_Cases;\npackage body P is\nbegin\n{REG}end P;\n"),
    );
    add(
        "registration in nested package",
        format!(
            "with AUnit.Test_Cases;\npackage body P is\npackage body Q is\nbegin\n{REG}end Q;\nend P;\n"
        ),
    );
    add("registration with label", body(&format!("<<Top>>\n{REG}")));
    add(
        "registration in comment and string",
        body(
            "--  Register_Routine (T, In_A_Comment'Access, \"no\");\nNote := \"Register_Routine (T, In_A_String'Access, \"\"no\"\")\";\nnull;\n",
        ),
    );
    add(
        "registration after loop label",
        body(&format!(
            "Outer : for I in 1 .. 3 loop\n{REG}end loop Outer;\n"
        )),
    );
    add(
        "registration unicode in comment",
        body(&format!("-- \u{fc}ber caf\u{e9}\n{REG}")),
    );
    add(
        "registration unicode in string",
        body("Register_Routine (T, Loads'Access, \"caf\u{e9}\");\n"),
    );
    add("crlf line endings", body(REG).replace('\n', "\r\n"));
    add("tabs", body(REG).replace("    ", "\t"));
    add("no trailing newline", body(REG).trim_end().to_string());

    // ---- ordinary Ada the declared subset admits --------------------------
    for (name, declaration) in [
        ("object", "X : Integer;"),
        ("object init", "X : Integer := 1;"),
        ("object list", "X, Y, Z : Integer := 0;"),
        ("constant", "X : constant Integer := 1;"),
        ("number", "X : constant := 5;"),
        ("aliased", "X : aliased Integer;"),
        ("aliased constant", "X : aliased constant Integer := 1;"),
        ("range subtype", "X : Integer range 1 .. 10 := 1;"),
        ("based literal", "Mask : constant Natural := 16#FF#;"),
        ("based real", "Mask : constant Float := 2#1.1#E4;"),
        ("real literal", "R : constant Float := 3.5E-2;"),
        ("underscore literal", "N : constant Natural := 1_000_000;"),
        ("char literals", "Tick : constant Character := ''';\nParen : constant Character := '(';"),
        ("string literal", "S : constant String := \"abc\";"),
        ("exception", "Empty : exception;"),
        ("exception renames", "E : exception renames Ada.IO_Exceptions.End_Error;"),
        ("object renames", "X : Integer renames Y;"),
        ("object renames typed", "X : Integer renames Y.Z;"),
        ("package renames", "package P renames Q.R;"),
        ("subprogram renames", "procedure P renames Q;"),
        ("function renames", "function F return Integer renames G;"),
        ("subtype", "subtype Small is Natural range 0 .. 10;"),
        ("subtype plain", "subtype Small is Natural;"),
        ("subtype constraint", "subtype Short is String (1 .. 5);"),
        ("subtype not null", "subtype P is not null access Integer;"),
        ("integer type", "type T is range 1 .. 10;"),
        ("modular type", "type T is mod 256;"),
        ("float type", "type T is digits 6;"),
        ("float range type", "type T is digits 6 range 0.0 .. 1.0;"),
        ("fixed type", "type T is delta 0.1 range 0.0 .. 1.0;"),
        ("decimal type", "type T is delta 0.01 digits 10;"),
        ("enumeration", "type Color is (Red, Green, Blue);"),
        ("enumeration char", "type Letter is ('a', 'b');"),
        ("array", "type A is array (1 .. 3) of Integer;"),
        ("array two dim", "type A is array (1 .. 3, 1 .. 4) of Integer;"),
        ("array unconstrained", "type A is array (Positive range <>) of Integer;"),
        ("array of access", "type A is array (Color) of access Integer;"),
        ("array aliased", "type A is array (1 .. 3) of aliased Integer;"),
        ("anonymous array object", "Table : constant array (1 .. 3) of Natural := (1, 2, 3);"),
        ("record", "type R is record X : Integer; Y : Float := 1.0; end record;"),
        ("null record", "type R is null record;"),
        ("tagged null", "type R is tagged null record;"),
        ("tagged record", "type R is tagged record X : Integer; end record;"),
        ("limited record", "type R is limited record X : Integer; end record;"),
        ("abstract tagged", "type R is abstract tagged null record;"),
        ("abstract tagged limited", "type R is abstract tagged limited null record;"),
        ("derived null", "type D is new Base with null record;"),
        ("derived record", "type D is new Base with record X : Integer; end record;"),
        ("derived private", "type D is new Base with private;"),
        ("derived plain", "type D is new Integer;"),
        ("derived range", "type D is new Integer range 1 .. 10;"),
        ("private type", "type P is private;"),
        ("limited private type", "type P is limited private;"),
        ("incomplete type", "type Node;"),
        ("discriminated", "type R (N : Natural := 0) is record X : String (1 .. N); end record;"),
        ("unknown discriminant", "type R (<>) is private;"),
        ("discriminant access", "type R (P : access Integer) is limited null record;"),
        ("variant", "type R (K : Boolean) is record case K is when True => A : Integer; when False => null; end case; end record;"),
        ("variant others", "type R (K : Natural) is record case K is when 0 => A : Integer; when others => B : Float; end case; end record;"),
        ("component list null", "type R is record null; end record;"),
        ("access type", "type P is access Integer;"),
        ("access all", "type P is access all Integer;"),
        ("access constant", "type P is access constant Integer;"),
        ("not null access", "type P is not null access Integer;"),
        ("access procedure", "type P is access procedure (X : Integer);"),
        ("access function", "type P is access function (X : Integer) return Integer;"),
        ("access protected procedure", "type P is access protected procedure;"),
        ("access object", "X : access Integer;"),
        ("access object constant", "X : access constant Integer := null;"),
        ("generic instantiation procedure", "procedure Swap is new Generic_Swap (Integer);"),
        ("generic instantiation function", "function Max is new Generic_Max (Integer);"),
        ("generic instantiation package", "package Lists is new Generic_Lists (Integer);"),
        ("generic instantiation named", "package Lists is new Generic_Lists (Element => Integer, Size => 10);"),
        ("generic instantiation aspect", "package Lists is new Generic_Lists (Integer) with Preelaborate;"),
        ("nested package decl", "package Inner is X : Integer; end Inner;"),
        ("nested package with private", "package Inner is X : Integer; private Y : Integer; end Inner;"),
        ("nested package body", "package body Inner is end Inner;"),
        ("nested package body begin", "package body Inner is begin null; end Inner;"),
        ("subprogram decl", "procedure P (X : Integer);"),
        ("function decl", "function F (X : Integer) return Integer;"),
        ("function decl no params", "function F return Integer;"),
        ("function decl operator", "function \"=\" (L, R : T) return Boolean;"),
        ("function decl operator plus", "function \"+\" (L, R : T) return T;"),
        ("abstract subprogram", "procedure P (X : in out T) is abstract;"),
        ("abstract function", "function F (X : T) return Integer is abstract;"),
        ("null procedure", "procedure P (X : in out T) is null;"),
        ("expression function", "function F (X : Integer) return Integer is (X + 1);"),
        ("expression function aspect", "function F return Integer is (1) with Inline;"),
        ("conditional expression function", "function F (X : Integer) return Integer is (if X > 0 then X else -X);"),
        ("subprogram with defaults", "procedure P (X : Integer := 0; Y : in Float := 1.0);"),
        ("subprogram access param", "procedure P (X : access Integer; Y : access constant Integer);"),
        ("subprogram not null param", "procedure P (X : not null access Integer);"),
        ("subprogram aliased param", "procedure P (X : aliased in out Integer);"),
        ("subprogram out param", "procedure P (X : out Integer);"),
        ("subprogram class param", "procedure P (X : in out Root'Class);"),
        ("function access result", "function F return access Integer;"),
        ("function not null result", "function F return not null access Integer;"),
        ("function aspect", "function F return Integer with Pre => True, Post => F'Result > 0;"),
        ("procedure aspect", "procedure P with Inline;"),
        ("procedure aspect mark tick", "procedure P with Pre'Class => True;"),
        ("object aspect", "X : Integer := 1 with Volatile;"),
        ("type aspect", "type T is range 1 .. 10 with Size => 8;"),
        ("subtype aspect", "subtype S is Integer range 1 .. 10 with Static_Predicate => S /= 5;"),
        ("component aspect", "type R is record X : Integer with Atomic; end record;"),
        ("pragma import", "procedure P;\npragma Import (C, P, \"p\");"),
        ("pragma inline", "procedure P;\npragma Inline (P);"),
        ("pragma preelaborate", "pragma Preelaborate;"),
        ("use clause", "use Ada.Text_IO;"),
        ("use type clause", "use type Ada.Calendar.Time;"),
        ("use all type", "use all type Ada.Calendar.Time;"),
        ("use list", "use A, B.C, D;"),
        ("attribute type", "type A is array (X'Range) of Integer;"),
        ("attribute object", "X : Integer := Integer'Last - Integer'First;"),
        ("attribute with arg", "X : Integer := Integer'Max (1, 2);"),
        ("attribute image", "S : String := Integer'Image (5);"),
        ("attribute class", "type T is access all Root'Class;"),
        ("qualified expression object", "X : Integer := Integer'(5);"),
        ("aggregate object", "X : Arr := (1, 2, 3);"),
        ("aggregate named", "X : Arr := (1 => 1, 2 => 2);"),
        ("aggregate others", "X : Arr := (others => 0);"),
        ("aggregate choice list", "X : Arr := (1 | 2 => 1, others => 0);"),
        ("aggregate range", "X : Arr := (1 .. 3 => 0);"),
        ("aggregate box", "X : Rec := (A => <>, B => 1);"),
        ("aggregate extension", "X : Derived := (Base with Z => 1);"),
        ("aggregate extension null", "X : Derived := (Base with null record);"),
        ("aggregate null record", "X : Empty := (null record);"),
        ("aggregate nested", "X : Matrix := ((1, 2), (3, 4));"),
        ("allocator", "P : Ptr := new Integer;"),
        ("allocator init", "P : Ptr := new Integer'(5);"),
        ("allocator constraint", "P : Ptr := new String (1 .. 5);"),
        ("allocator discriminant", "P : Ptr := new Rec (N => 5);"),
        ("null init", "P : Ptr := null;"),
        ("conditional expression", "X : Integer := (if A then 1 else 2);"),
        ("conditional elsif", "X : Integer := (if A then 1 elsif B then 2 else 3);"),
        ("conditional no else", "X : Boolean := (if A then B);"),
        ("case expression", "X : Integer := (case Y is when 1 => 1, when others => 0);"),
        ("quantified expression", "X : Boolean := (for all I in 1 .. 3 => I > 0);"),
        ("quantified of", "X : Boolean := (for all E of Arr => E > 0);"),
        ("membership", "X : Boolean := Y in 1 .. 10;"),
        ("not membership", "X : Boolean := Y not in 1 .. 10;"),
        ("membership list", "X : Boolean := Y in 1 | 3 | 5 .. 9;"),
        ("membership subtype", "X : Boolean := Y in Small;"),
        ("and then", "X : Boolean := A and then B or else C;"),
        ("logic ops", "X : Boolean := A and B;\nY : Boolean := A or B;\nZ : Boolean := A xor B;"),
        ("relational ops", "X : Boolean := A = B or A /= B or A < B or A <= B or A > B or A >= B;"),
        ("arith ops", "X : Integer := A + B - C * D / E mod F rem G;"),
        ("exponent", "X : Integer := A ** 2;"),
        ("unary ops", "X : Integer := -A;\nY : Integer := +A;\nZ : Integer := abs A;\nB : Boolean := not A;"),
        ("concatenation", "S : String := \"a\" & \"b\" & 'c';"),
        ("function call object", "X : Integer := F (1, 2);"),
        ("function call named", "X : Integer := F (A => 1, B => 2);"),
        ("function call nested", "X : Integer := F (G (H (1)));"),
        ("indexed", "X : Integer := Table (1);"),
        ("indexed multi", "X : Integer := Table (1, 2);"),
        ("slice", "S : String := Text (1 .. 3);"),
        ("slice range attr", "S : String := Text (Text'First .. Text'Last);"),
        ("selected", "X : Integer := Rec.Field;"),
        ("selected all", "X : Integer := Ptr.all;"),
        ("selected all field", "X : Integer := Ptr.all.Field;"),
        ("implicit deref call", "X : Integer := Ptr.Field (1);"),
        ("operator call", "X : Integer := \"+\" (1, 2);"),
        ("operator selected", "X : Integer := Standard.\"+\" (1, 2);"),
        ("char function", "C : Character := Character'Val (65);"),
        ("tick in call", "X : Integer := Items (1)'Length;"),
        ("range attribute", "type A is array (Items'Range) of Integer;"),
        ("range attribute dim", "type A is array (Items'Range (1)) of Integer;"),
        ("delta attribute", "X : Float := Fixed'Delta;"),
        ("digits attribute", "X : Integer := Float'Digits;"),
        ("mod attribute", "X : Integer := Natural'Mod (5);"),
        ("access attribute", "P : Ptr := X'Access;"),
        ("unchecked access attribute", "P : Ptr := X'Unchecked_Access;"),
        ("address attribute", "A : System.Address := X'Address;"),
        ("component clause free", "Count : Natural := 0;"),
    ] {
        add(&format!("ordinary: {name}"), decl(declaration));
        add(&format!("ordinary local: {name}"), local_decl(declaration));
    }
    for (name, statement) in [
        ("null", "null;"),
        ("assignment", "X := 1;"),
        ("assignment indexed", "Table (1) := 2;"),
        ("assignment selected", "Rec.Field := 3;"),
        ("assignment deref", "Ptr.all := 4;"),
        ("assignment aggregate", "X := (1, 2);"),
        ("call", "Do_It;"),
        ("call args", "Do_It (1, 2);"),
        ("call named", "Do_It (A => 1, B => 2);"),
        ("call mixed", "Do_It (1, B => 2);"),
        ("call selected", "Pkg.Do_It (1);"),
        ("call deep selected", "A.B.C.Do_It (1);"),
        ("call nested", "F (G (H (1)));"),
        ("call string", "Put_Line (\"hello\");"),
        ("call with attribute", "Put_Line (Integer'Image (X));"),
        ("call concatenation", "Put_Line (\"a\" & \"b\");"),
        ("call box", "Do_It (A => <>);"),
        ("if", "if A then null; end if;"),
        ("if else", "if A then null; else null; end if;"),
        ("if elsif", "if A then null; elsif B then null; elsif C then null; else null; end if;"),
        ("if compound", "if A and then B or else C then null; end if;"),
        ("if in", "if X in 1 .. 3 then null; end if;"),
        ("case", "case X is when 1 => null; when 2 | 3 => null; when others => null; end case;"),
        ("case range", "case X is when 1 .. 3 => null; when others => null; end case;"),
        ("case subtype", "case X is when Small => null; when others => null; end case;"),
        ("case multiple statements", "case X is when 1 => null; null; when others => null; end case;"),
        ("while", "while A loop null; end loop;"),
        ("for", "for I in 1 .. 3 loop null; end loop;"),
        ("for reverse", "for I in reverse 1 .. 3 loop null; end loop;"),
        ("for subtype", "for I in Small loop null; end loop;"),
        ("for range attr", "for I in Table'Range loop null; end loop;"),
        ("for of", "for E of Table loop null; end loop;"),
        ("for of reverse", "for E of reverse Table loop null; end loop;"),
        ("for typed", "for I : Integer in 1 .. 3 loop null; end loop;"),
        ("for in subtype range", "for I in Small range 1 .. 2 loop null; end loop;"),
        ("loop", "loop exit; end loop;"),
        ("exit when", "loop exit when A; end loop;"),
        ("exit named", "Outer : loop exit Outer when A; end loop Outer;"),
        ("loop label", "Outer : while A loop null; end loop Outer;"),
        ("block", "begin null; end;"),
        ("block declare", "declare X : Integer; begin null; end;"),
        ("block named", "Blk : declare X : Integer; begin null; end Blk;"),
        ("block handler", "begin null; exception when others => null; end;"),
        ("block handler choice", "begin null; exception when Constraint_Error | Program_Error => null; end;"),
        ("block handler named", "begin null; exception when E : Constraint_Error => null; end;"),
        ("block handler raise", "begin null; exception when others => raise; end;"),
        ("block handler several", "begin null; exception when A => null; when B => null; when others => null; end;"),
        ("raise", "raise Constraint_Error;"),
        ("raise with", "raise Constraint_Error with \"bad\";"),
        ("raise selected", "raise Pkg.Error;"),
        ("raise bare", "begin null; exception when others => raise; end;"),
        ("return", "return;"),
        ("goto and label", "goto Done; <<Done>> null;"),
        ("label stacked", "<<A>> <<B>> null;"),
        ("pragma statement", "pragma Assert (X > 0);"),
        ("pragma assert message", "pragma Assert (X > 0, \"positive\");"),
        ("pragma debug", "pragma Debug (Put_Line (\"x\"));"),
        ("nested block in if", "if A then declare X : Integer; begin null; end; end if;"),
        ("declare with subprogram", "declare procedure Q is begin null; end Q; begin Q; end;"),
        ("declare with package", "declare package Q is end Q; begin null; end;"),
        ("declare with type", "declare type T is range 1 .. 3; begin null; end;"),
        ("extended return", "declare function F return Integer is begin return R : Integer := 1 do null; end return; end F; begin null; end;"),
        ("extended return plain", "declare function F return Integer is begin return R : Integer; end F; begin null; end;"),
        ("extended return aliased", "declare function F return Integer is begin return R : aliased Integer := 1; end F; begin null; end;"),
        ("return expression", "declare function F return Integer is begin return 1; end F; begin null; end;"),
    ] {
        add(&format!("statement: {name}"), body(&format!("{statement}\n")));
    }

    // ---- nesting ceiling --------------------------------------------------
    for levels in [8, 20, 28, 30, 31, 32, 33, 34, 40, 60, 100] {
        add(
            &format!("depth parens {levels}"),
            body(&format!(
                "X := {}1{};\n",
                "(".repeat(levels),
                ")".repeat(levels)
            )),
        );
    }
    for levels in [8, 20, 22, 23, 24, 25, 26, 30, 50] {
        add(
            &format!("depth nested calls {levels}"),
            body(&format!(
                "X := {}1{};\n",
                "F (".repeat(levels),
                ")".repeat(levels)
            )),
        );
        add(
            &format!("depth nested call statements {levels}"),
            body(&format!(
                "{}1{};\n",
                "F (".repeat(levels),
                ")".repeat(levels)
            )),
        );
    }
    for levels in [8, 40, 45, 46, 47, 48, 49, 50, 60, 100] {
        let open = "if A then\n";
        let close = "end if;\n";
        add(
            &format!("depth nested if {levels}"),
            body(&format!(
                "{}null;\n{}",
                open.repeat(levels),
                close.repeat(levels)
            )),
        );
        add(
            &format!("depth nested loops {levels}"),
            body(&format!(
                "{}null;\n{}",
                "loop\n".repeat(levels),
                "end loop;\n".repeat(levels)
            )),
        );
        add(
            &format!("depth nested blocks {levels}"),
            body(&format!(
                "{}null;\n{}",
                "begin\n".repeat(levels),
                "end;\n".repeat(levels)
            )),
        );
        add(
            &format!("depth nested case {levels}"),
            body(&format!(
                "{}null;\n{}",
                "case X is when others =>\n".repeat(levels),
                "end case;\n".repeat(levels)
            )),
        );
        add(
            &format!("depth nested declare {levels}"),
            body(&format!(
                "{}null;\n{}",
                "declare\nbegin\n".repeat(levels),
                "end;\n".repeat(levels)
            )),
        );
        add(
            &format!("depth nested handlers {levels}"),
            body(&format!(
                "{}null;\n{}",
                "begin\nnull;\nexception when others =>\n".repeat(levels),
                "end;\n".repeat(levels)
            )),
        );
    }
    for levels in [10, 40, 45, 46, 47, 48, 49, 50, 60, 100] {
        add(
            &format!("depth nested packages {levels}"),
            package_with(&format!(
                "{}{}",
                "package body Q is\n".repeat(levels),
                "end Q;\n".repeat(levels)
            )),
        );
        add(
            &format!("depth nested subprograms {levels}"),
            package_with(&format!(
                "{}null;\n{}",
                "procedure Q is\nbegin\n".repeat(levels),
                "end Q;\n".repeat(levels)
            )),
        );
        add(
            &format!("depth nested decl subprograms {levels}"),
            package_with(&format!(
                "{}{}",
                "procedure Q is\n".repeat(levels),
                "begin\nnull;\nend Q;\n".repeat(levels)
            )),
        );
    }
    for levels in [10, 30, 45, 46, 47, 48, 60, 100] {
        add(
            &format!("depth nested records {levels}"),
            decl(&format!(
                "type R is record\n{}end record;",
                nested("A : record\n", "end record;\n", levels)
            )),
        );
        add(
            &format!("depth nested variants {levels}"),
            decl(&format!(
                "type R (K : Natural) is record\n{}null;\n{}end record;",
                "case K is when others =>\n".repeat(levels),
                "end case;\n".repeat(levels)
            )),
        );
    }
    for levels in [10, 20, 30, 31, 32, 33, 40, 100] {
        add(
            &format!("depth nested aggregates {levels}"),
            decl(&format!(
                "X : Arr := {}1{};",
                "(1, ".repeat(levels),
                ")".repeat(levels)
            )),
        );
        add(
            &format!("depth nested conditionals {levels}"),
            decl(&format!(
                "X : Integer := {}1{};",
                "(if A then ".repeat(levels),
                " else 2)".repeat(levels)
            )),
        );
        add(
            &format!("depth nested indexing {levels}"),
            body(&format!(
                "X := T {}1{};\n",
                "(T ".repeat(levels),
                ")".repeat(levels)
            )),
        );
        add(
            &format!("depth nested qualified {levels}"),
            body(&format!(
                "X := {}1{};\n",
                "T'(".repeat(levels),
                ")".repeat(levels)
            )),
        );
    }
    add(
        "depth exactly at ceiling is a plain parens ceiling",
        body(&format!(
            "X := {}1{};\n",
            "(".repeat(MAX_DEPTH / 3),
            ")".repeat(MAX_DEPTH / 3)
        )),
    );
    add(
        "depth error after ceiling",
        body(&format!(
            "X := {}1{};\nX := ;\n",
            "(".repeat(100),
            ")".repeat(100)
        )),
    );
    add(
        "depth ceiling before error",
        body(&format!(
            "X := ;\nX := {}1{};\n",
            "(".repeat(100),
            ")".repeat(100)
        )),
    );

    // ---- a realistic AUnit suite -----------------------------------------
    add(
        "realistic suite",
        "with AUnit.Assertions;\nwith AUnit.Test_Cases;\nuse AUnit.Assertions;\n\
         package body Catalog_Tests is\n\
         type Test_Case is new AUnit.Test_Cases.Test_Case with null record;\n\
         subtype Small is Natural range 0 .. 10;\n\
         Table : constant array (1 .. 3) of Natural := (1, 2, 3);\n\
         Mask  : constant Natural := 16#FF#;\n\
         Tick  : constant Character := ''';\n\
         Empty : exception;\n\
         function Score (Value : Natural := 0) return Natural is\n\
         begin\n\
            if Value > 10 then\n   return 10;\n   elsif Value in 1 .. 9 then\n\
               return Value;\n   else\n      return 0;\n   end if;\n\
         end Score;\n\
         procedure Loads (T : in out AUnit.Test_Cases.Test_Case'Class) is\n\
            Total : Natural := 0;\n\
         begin\n\
            for Index in Table'Range loop\n      Total := Total + Table (Index);\n   end loop;\n\
            case Total is\n      when 0 => null;\n      when others => Assert (True, \"loads\");\n\
            end case;\n\
            declare\n      Local : constant Natural := Total;\n   begin\n\
               Assert (Local > 0, \"positive\");\n   end;\n\
         exception\n   when Empty => null;\n   when others => raise;\n\
         end Loads;\n\
         procedure Register_Tests (T : in out Test_Case) is\n\
         begin\n   Register_Routine (T, Loads'Access, \"loads the catalog\");\n\
         end Register_Tests;\n\
         end Catalog_Tests;\n"
            .to_string(),
    );
    add(
        "realistic suite with overriding and type extension",
        "with AUnit.Test_Cases;\nwith AUnit.Simple_Test_Cases;\n\
         package body Suite is\n\
         overriding function Name (T : Test_Case) return AUnit.Message_String is\n\
         begin\n   return AUnit.Format (\"suite\");\nend Name;\n\
         overriding procedure Set_Up (T : in out Test_Case) is\nbegin\n   null;\nend Set_Up;\n\
         procedure Check (T : in out AUnit.Test_Cases.Test_Case'Class) is\nbegin\n\
            AUnit.Assertions.Assert (Condition => True, Message => \"ok\");\nend Check;\n\
         overriding procedure Register_Tests (T : in out Test_Case) is\n\
            use AUnit.Test_Cases.Registration;\n\
         begin\n\
            Register_Routine (T, Check'Access, \"check\");\n\
            Register_Routine (Test => T, Routine => Check'Access, Name => \"named\");\n\
         end Register_Tests;\nend Suite;\n"
            .to_string(),
    );
    corpus.extend(stress_corpus());
    corpus
}

/// Round-two corpus: every reserved word in every name position, lexical
/// oddities, expression and declaration near-misses, and refusal precedence.
fn stress_corpus() -> Vec<(String, String)> {
    let mut corpus: Vec<(String, String)> = Vec::new();
    let mut add = |name: &str, source: String| corpus.push((name.to_string(), source));

    let words = [
        "abort",
        "abs",
        "abstract",
        "accept",
        "access",
        "aliased",
        "all",
        "and",
        "array",
        "at",
        "begin",
        "body",
        "case",
        "constant",
        "declare",
        "delay",
        "delta",
        "digits",
        "do",
        "else",
        "elsif",
        "end",
        "entry",
        "exception",
        "exit",
        "for",
        "function",
        "generic",
        "goto",
        "if",
        "in",
        "is",
        "limited",
        "loop",
        "mod",
        "new",
        "not",
        "null",
        "of",
        "or",
        "others",
        "out",
        "package",
        "pragma",
        "private",
        "procedure",
        "protected",
        "raise",
        "range",
        "record",
        "rem",
        "renames",
        "requeue",
        "return",
        "reverse",
        "select",
        "separate",
        "subtype",
        "tagged",
        "task",
        "terminate",
        "then",
        "type",
        "until",
        "use",
        "when",
        "while",
        "with",
        "xor",
    ];
    for word in words {
        add(
            &format!("word object {word}"),
            decl(&format!("{word} : Integer;")),
        );
        add(
            &format!("word type {word}"),
            decl(&format!("type {word} is range 1 .. 3;")),
        );
        add(
            &format!("word procedure {word}"),
            decl(&format!("procedure {word} is begin null; end {word};")),
        );
        add(
            &format!("word assign {word}"),
            body(&format!("{word} := 1;\n")),
        );
        add(&format!("word call {word}"), body(&format!("{word};\n")));
        add(
            &format!("word call args {word}"),
            body(&format!("{word} (1);\n")),
        );
        add(
            &format!("word expression {word}"),
            body(&format!("X := {word};\n")),
        );
        add(
            &format!("word selector {word}"),
            body(&format!("X := Pkg.{word};\n")),
        );
        add(
            &format!("word selector call {word}"),
            body(&format!("Pkg.{word} (1);\n")),
        );
        add(
            &format!("word prefix {word}"),
            body(&format!("{word}.X := 1;\n")),
        );
        add(
            &format!("word attribute {word}"),
            body(&format!("X := Y'{word};\n")),
        );
        add(
            &format!("word attribute arg {word}"),
            body(&format!("X := Y'{word} (1);\n")),
        );
        add(
            &format!("word pragma {word}"),
            decl(&format!("pragma {word};")),
        );
        add(
            &format!("word pragma args {word}"),
            decl(&format!("pragma {word} (A, B);")),
        );
        add(
            &format!("word with {word}"),
            format!("with {word};\npackage body P is\nend P;\n"),
        );
        add(
            &format!("word with selected {word}"),
            format!("with Ada.{word};\npackage body P is\nend P;\n"),
        );
        add(
            &format!("word package {word}"),
            format!("package body {word} is\nend {word};\n"),
        );
        add(
            &format!("word aspect {word}"),
            decl(&format!("procedure Q with {word};")),
        );
        add(
            &format!("word aspect value {word}"),
            decl(&format!("procedure Q with Inline => {word};")),
        );
        add(
            &format!("word parameter {word}"),
            decl(&format!("procedure Q ({word} : Integer);")),
        );
        add(
            &format!("word named association {word}"),
            body(&format!("Do_It ({word} => 1);\n")),
        );
        add(
            &format!("word label {word}"),
            body(&format!("<<{word}>> null;\n")),
        );
        add(
            &format!("word loop name {word}"),
            body(&format!("{word} : loop exit; end loop {word};\n")),
        );
        add(
            &format!("word enumeration {word}"),
            decl(&format!("type E is ({word}, Other);")),
        );
        add(
            &format!("word component {word}"),
            decl(&format!("type R is record {word} : Integer; end record;")),
        );
        add(
            &format!("word exception {word}"),
            decl(&format!("{word} : exception;")),
        );
        add(
            &format!("word subtype {word}"),
            decl(&format!("subtype {word} is Integer;")),
        );
        add(
            &format!("word case choice {word}"),
            body(&format!(
                "case X is when {word} => null; when others => null; end case;\n"
            )),
        );
        add(
            &format!("word for loop {word}"),
            body(&format!("for {word} in 1 .. 3 loop null; end loop;\n")),
        );
        add(
            &format!("word quantified {word}"),
            body(&format!("X := (for all {word} in 1 .. 3 => True);\n")),
        );
        add(
            &format!("word handler {word}"),
            body(&format!("null;\nexception when {word} => null;\n")),
        );
        add(
            &format!("word return object {word}"),
            decl(&format!(
                "function F return Integer is begin return {word} : Integer := 1; end F;"
            )),
        );
    }

    // ---- lexical oddities -------------------------------------------------
    for (name, statement) in [
        ("char qualified", "X := Character'('a');"),
        ("char range", "for C in 'a' .. 'z' loop null; end loop;"),
        ("char pos", "X := Character'Pos ('A');"),
        ("char pos qualified", "X := Standard.Character'Pos ('A');"),
        ("char val", "C := Character'Val (65);"),
        ("char literal equals", "if C = 'a' then null; end if;"),
        ("char tick tick", "C := ''';"),
        ("char paren", "C := '(';"),
        ("char space", "C := ' ';"),
        ("char double quote", "C := '\"';"),
        ("char dash", "C := '-';"),
        ("char unicode", "C := '\u{e9}';"),
        ("tick after all", "X := Ptr.all'Size;"),
        ("tick after paren", "X := F (1)'Size;"),
        ("tick after string", "X := \"+\"'Access;"),
        ("tick range", "for I in A'Range loop null; end loop;"),
        ("tick first last", "X := A'First + A'Last;"),
        ("tick class", "X := T'Class'Size;"),
        ("tick before paren", "X := T'(1);"),
        ("tick in call", "P (T'(1), T'Size);"),
        ("tick access in assignment", "P := X'Access;"),
        ("number dots", "for I in 1..2 loop null; end loop;"),
        ("number exp", "X := 1E5;"),
        ("number real exp", "X := 1.0e+5;"),
        ("number real exp neg", "X := 1.5E-3;"),
        ("number based", "X := 16#FF#;"),
        ("number based exp", "X := 2#1010#E4;"),
        ("number based real", "X := 16#F.F#E2;"),
        ("number underscores", "X := 1_000.0_1;"),
        ("number based underscores", "X := 10#1_0#;"),
        ("number lower base", "X := 16#ff#;"),
        ("number identifier suffix", "X := 1 e;"),
        ("number trailing dot", "X := 1.;"),
        ("number leading dot", "X := .5;"),
        ("string dashes", "S := \"-- not a comment\";"),
        ("string doubled", "S := \"a\"\"b\";"),
        ("string empty", "S := \"\";"),
        ("string apostrophe", "S := \"it's\";"),
        ("string hash", "S := \"#if\";"),
        ("string percent", "S := \"%x%\";"),
        ("string dollar", "S := \"$x\";"),
        ("string bracket", "S := \"[x]\";"),
        ("string unicode", "S := \"caf\u{e9}\";"),
        ("string long", &format!("S := \"{}\";", "x".repeat(2000))),
        ("operator string", "X := \"+\" (1, 2);"),
        ("operator string and", "X := \"and\" (A, B);"),
        ("comment trailing", "null; -- done"),
        ("comment unicode", "null; -- caf\u{e9}"),
        ("comment dashes", "null; ----------"),
        ("comment immediately", "null;--x"),
        ("minus minus ambiguity", "X := A - -B;"),
        ("minus minus ambiguity spaced", "X := A - - B;"),
        ("keyword case mix", "If A Then Null; End If;"),
        ("keyword upper", "IF A THEN NULL; END IF;"),
        ("tabs and newlines", "if\tA\n\tthen\r\n\tnull;\r\nend\tif;"),
        ("form feed", "null;\u{c}null;"),
        ("vertical tab", "null;\u{b}null;"),
        ("nul byte", "null;\u{0}null;"),
        ("bom", "\u{feff}null;"),
        ("nbsp", "null;\u{a0}null;"),
        ("underscore leading", "_X := 1;"),
        ("underscore trailing", "X_ := 1;"),
        ("underscore double", "X__Y := 1;"),
        ("identifier digits", "X1_2 := 1;"),
        ("identifier unicode", "\u{e9}tat := 1;"),
        ("identifier long", &format!("{} := 1;", "A".repeat(500))),
    ] {
        add(&format!("lexical: {name}"), body(&format!("{statement}\n")));
    }
    add("lexical: bom file", format!("\u{feff}{}", body(REG)));
    add("lexical: nul file", format!("{}\u{0}", body(REG)));
    add("lexical: lone cr", body(REG).replace('\n', "\r"));
    add(
        "lexical: comment at eof no newline",
        format!("{}-- eof", body(REG)),
    );
    add(
        "lexical: comment between every token",
        "with -- a\n AUnit -- b\n . -- c\n Test_Cases -- d\n ; -- e\n package -- f\n body -- g\n P -- h\n is -- i\n procedure -- j\n Q -- k\n is -- l\n begin -- m\n Register_Routine -- n\n ( -- o\n T -- p\n , -- q\n L -- r\n ' -- s\n Access -- t\n , -- u\n \"x\" -- v\n ) -- w\n ; -- x\n end -- y\n Q -- z\n ; -- A\n end -- B\n P -- C\n ; -- D\n"
            .to_string(),
    );

    // ---- expressions ------------------------------------------------------
    for (name, expression) in [
        ("power chain", "A ** B ** C"),
        ("power paren", "A ** (B ** C)"),
        ("plus minus", "A + - B"),
        ("minus unary", "- A"),
        ("double unary", "- - A"),
        ("abs not", "abs not A"),
        ("not abs", "not abs A"),
        ("not not", "not not A"),
        ("mul chain", "A * B / C mod D rem E"),
        ("unary then mul", "- A * B"),
        ("mul unary", "A * - B"),
        ("relation chain", "A < B < C"),
        ("relation membership", "A < B in C"),
        ("not in", "A not in B"),
        ("in then relation", "A in B = C"),
        ("and or mix", "A and B or C"),
        ("and then or else", "A and then B or else C"),
        ("or xor", "A or B xor C"),
        ("and xor", "A and B xor C"),
        ("and then plain", "A and then B"),
        ("or else plain", "A or else B"),
        ("and then chain", "A and then B and then C"),
        ("and and then", "A and B and then C"),
        ("catenation chain", "A & B & C"),
        ("catenation relation", "A & B = C"),
        ("membership choices", "A in B | C | D .. E"),
        ("membership range", "A in B .. C"),
        ("null", "null"),
        ("new", "new T"),
        ("new qualified", "new T'(1)"),
        ("new constrained", "new T (1 .. 3)"),
        ("new discriminant", "new T (N => 3)"),
        ("new not null", "new not null T"),
        ("new access", "new access T"),
        ("aggregate empty", "()"),
        ("aggregate single", "(1)"),
        ("aggregate named", "(A => 1, B => 2)"),
        ("aggregate choice list", "(A | B => 1)"),
        ("aggregate range", "(1 .. 3 => 0)"),
        ("aggregate others", "(others => 0)"),
        ("aggregate mixed", "(1, 2, others => 3)"),
        ("aggregate box", "(others => <>)"),
        ("aggregate with", "(Base with A => 1)"),
        ("aggregate with box", "(Base with others => <>)"),
        ("aggregate with positional", "(Base with 1, 2)"),
        ("aggregate null record", "(null record)"),
        ("aggregate with null record", "(Base with null record)"),
        ("aggregate nested named", "(A => (B => 1))"),
        ("aggregate call", "(F (1), G (2))"),
        ("aggregate qualified", "T'(A => 1)"),
        ("aggregate qualified positional", "T'(1, 2)"),
        ("if expression", "(if A then B else C)"),
        ("if expression elsif", "(if A then B elsif C then D else E)"),
        ("if expression no else", "(if A then B)"),
        (
            "case expression",
            "(case A is when 1 => B, when others => C)",
        ),
        (
            "case expression choices",
            "(case A is when 1 | 2 => B, when 3 .. 4 => C, when others => D)",
        ),
        ("quantified all", "(for all I in A => B)"),
        ("quantified some", "(for some I in A => B)"),
        ("quantified of", "(for all E of A => B)"),
        ("quantified reverse", "(for all I in reverse A => B)"),
        ("quantified typed", "(for all I : T in A => B)"),
        ("declare expression", "(declare X : T := 1; begin X)"),
        ("raise expression", "(raise Error)"),
        ("raise expression with", "(raise Error with \"m\")"),
        ("delta aggregate", "(Base with delta A => 1)"),
        ("reduce", "A'Reduce (\"+\", 0)"),
        ("target name", "@"),
        ("square aggregate", "[1, 2]"),
        ("iterated component", "(for I in 1 .. 3 => I)"),
        ("slice", "A (1 .. 3)"),
        ("slice attribute", "A (B'Range)"),
        ("slice subtype", "A (T range 1 .. 2)"),
        ("index multi", "A (1, 2, 3)"),
        ("call named", "F (A => 1)"),
        ("call mixed", "F (1, A => 2)"),
        ("call string", "F (\"x\")"),
        ("call box", "F (A => <>)"),
        ("call chain", "F (1) (2)"),
        ("call selected chain", "A.B (1).C (2).D"),
        ("call all", "A.all (1)"),
        ("selected string", "A.\"+\""),
        ("selected char", "A.'x'"),
        ("attribute chain", "A'Class'Size"),
        ("attribute call chain", "A'Image (1)'Length"),
        ("attribute in", "A'Range"),
        ("attribute access all", "A.all'Access"),
        ("qualified attribute", "T'(1)'Size"),
        ("character literal expression", "'x'"),
        ("string expression", "\"x\""),
        ("string call", "\"x\" (1)"),
        ("string selected", "\"x\".Y"),
        ("parenthesised string", "(\"x\")"),
        ("numeric literal", "1_000"),
        ("access expression", "X'Access"),
        ("unchecked", "X'Unchecked_Access"),
        ("address", "X'Address"),
        ("image", "Integer'Image (X)"),
        ("range attr expr", "T'Range (1)"),
        ("mod attr", "T'Mod (1)"),
        ("digits attr", "T'Digits"),
        ("delta attr", "T'Delta"),
    ] {
        add(
            &format!("expression: {name} decl"),
            decl(&format!("X : T := {expression};")),
        );
        add(
            &format!("expression: {name} stmt"),
            body(&format!("X := {expression};\n")),
        );
        add(
            &format!("expression: {name} call"),
            body(&format!("Do_It ({expression});\n")),
        );
        add(
            &format!("expression: {name} cond"),
            body(&format!("if {expression} then null; end if;\n")),
        );
        add(
            &format!("expression: {name} return"),
            decl(&format!(
                "function F return T is begin return {expression}; end F;"
            )),
        );
    }

    // ---- declarations -----------------------------------------------------
    for (name, declaration) in [
        ("interface list derived", "type T is new A and B with null record;"),
        ("interface list derived record", "type T is new A and B with record X : Integer; end record;"),
        ("derived with private interface", "type T is new A and B with private;"),
        ("limited derived", "type T is limited new A with null record;"),
        ("abstract limited derived", "type T is abstract limited new A with null record;"),
        ("abstract new", "type T is abstract new A with null record;"),
        ("abstract tagged limited record", "type T is abstract tagged limited record X : Integer; end record;"),
        ("tagged private", "type T is tagged private;"),
        ("tagged limited private", "type T is tagged limited private;"),
        ("abstract tagged private", "type T is abstract tagged private;"),
        ("limited private", "type T is limited private;"),
        ("private with discriminants", "type T (N : Natural) is private;"),
        ("private with unknown", "type T (<>) is private;"),
        ("derived with discriminants", "type T (N : Natural) is new A with null record;"),
        ("derived discriminant constraint", "type T is new A (1) with null record;"),
        ("derived range constraint", "type T is new A range 1 .. 3;"),
        ("derived digits constraint", "type T is new A digits 3;"),
        ("derived delta", "type T is new A delta 0.1;"),
        ("derived aspect", "type T is new A with null record with Size => 8;"),
        ("derived private aspect", "type T is new A with private with Size => 8;"),
        ("private aspect", "type T is private with Size => 8;"),
        ("array unconstrained generic", "type T is array (Integer range <>) of Integer;"),
        ("array unconstrained named", "type T is array (Positive range <>, Positive range <>) of Integer;"),
        ("array range subtype", "type T is array (Positive range 1 .. 3) of Integer;"),
        ("array of array", "type T is array (1 .. 3) of array (1 .. 2) of Integer;"),
        ("array of access constant", "type T is array (1 .. 3) of access constant Integer;"),
        ("array of not null", "type T is array (1 .. 3) of not null access Integer;"),
        ("array others", "type T is array (others) of Integer;"),
        ("array no bounds", "type T is array () of Integer;"),
        ("array component aliased constant", "type T is array (1 .. 3) of aliased constant Integer;"),
        ("object array anonymous", "X : array (1 .. 3) of Integer;"),
        ("object array anonymous init", "X : array (1 .. 3) of Integer := (others => 0);"),
        ("object access subprogram", "X : access procedure;"),
        ("object access function", "X : access function return Integer;"),
        ("object not null", "X : not null access Integer := Y;"),
        ("object constant array", "X : constant array (1 .. 3) of Integer := (1, 2, 3);"),
        ("object constant access", "X : constant access Integer := null;"),
        ("object constrained", "X : String (1 .. 5);"),
        ("object discriminated", "X : Rec (N => 1);"),
        ("object range", "X : Integer range 1 .. 3;"),
        ("object digits", "X : Float digits 3;"),
        ("object delta", "X : Fixed delta 0.1 range 0.0 .. 1.0;"),
        ("object class", "X : T'Class;"),
        ("object attribute type", "X : T'Base;"),
        ("object selected type", "X : Pkg.Sub.T;"),
        ("object unconstrained renames", "X : T renames Y;"),
        ("object not null renames", "X : not null access T renames Y;"),
        ("object no type renames", "X renames Y;"),
        ("object constant no type", "X : constant := 1.5;"),
        ("object constant list", "X, Y : constant Integer := 1;"),
        ("object aliased list", "X, Y : aliased Integer;"),
        ("object init list", "X, Y : Integer := 1;"),
        ("object missing type", "X : := 1;"),
        ("object missing colon", "X Integer;"),
        ("object missing semicolon", "X : Integer"),
        ("object keyword type", "X : null;"),
        ("exception list", "A, B : exception;"),
        ("exception renames", "A : exception renames B;"),
        ("number list", "A, B : constant := 1;"),
        ("subtype indication constraint", "subtype T is A (1 .. 3);"),
        ("subtype digits", "subtype T is A digits 3;"),
        ("subtype delta", "subtype T is A delta 0.1;"),
        ("subtype range box", "subtype T is A range <>;"),
        ("subtype access", "subtype T is access Integer;"),
        ("type range box", "type T is range <>;"),
        ("type mod expression", "type T is mod 2 ** 8;"),
        ("type range expression", "type T is range -2 ** 7 .. 2 ** 7 - 1;"),
        ("type enumeration single", "type T is (A);"),
        ("type enumeration empty", "type T is ();"),
        ("type enumeration char mixed", "type T is (A, 'b', C);"),
        ("type enumeration aspect", "type T is (A, B) with Size => 8;"),
        ("type access aspect", "type T is access Integer with Storage_Size => 0;"),
        ("type access procedure params", "type T is access procedure (A : Integer; B : out Integer);"),
        ("type access function result", "type T is access function return access Integer;"),
        ("type not null access procedure", "type T is not null access procedure;"),
        ("type access protected function", "type T is access protected function (A : Integer) return Integer;"),
        ("type incomplete tagged", "type T is tagged;"),
        ("type incomplete discriminated", "type T (N : Natural);"),
        ("type discriminant defaults", "type T (A : Integer := 1; B : Boolean := True) is null record;"),
        ("type discriminant access anon", "type T (A : access Integer) is limited null record;"),
        ("type discriminant not null", "type T (A : not null access Integer) is limited null record;"),
        ("type record aliased", "type T is record A : aliased Integer; end record;"),
        ("type record constant", "type T is record A : constant Integer := 1; end record;"),
        ("type record default", "type T is record A : Integer := 1; end record;"),
        ("type record list", "type T is record A, B : Integer; end record;"),
        ("type record empty", "type T is record end record;"),
        ("type record null then field", "type T is record null; A : Integer; end record;"),
        ("type record variant nested", "type T (K : Boolean) is record case K is when True => case J is when others => null; end case; when False => null; end case; end record;"),
        ("type record variant fields after", "type T (K : Boolean) is record A : Integer; case K is when True => B : Integer; when False => null; end case; end record;"),
        ("type record variant choices", "type T (K : Natural) is record case K is when 1 | 2 => A : Integer; when 3 .. 4 => B : Integer; when others => null; end case; end record;"),
        ("type record pragma", "type T is record pragma Convention (C, T); A : Integer; end record;"),
        ("type record pragma after", "type T is record A : Integer; pragma Atomic (A); end record;"),
        ("type record access anon", "type T is record A : access Integer; end record;"),
        ("type record not null", "type T is record A : not null access Integer := X; end record;"),
        ("type record array anon", "type T is record A : array (1 .. 3) of Integer; end record;"),
        ("type record end name", "type T is record A : Integer; end record T;"),
        ("package nested spec private", "package Q is type T is private; private type T is new Integer; end Q;"),
        ("package nested spec generic child", "package Q.R is end Q.R;"),
        ("package nested spec aspect", "package Q with Pure is end Q;"),
        ("package nested spec empty", "package Q is end Q;"),
        ("package nested spec pragma", "package Q is pragma Pure; end Q;"),
        ("package nested spec subprogram", "package Q is procedure P; end Q;"),
        ("package nested spec use", "package Q is use A; end Q;"),
        ("package nested spec no end name", "package Q is end;"),
        ("package nested spec wrong end", "package Q is end R;"),
        ("package nested body private", "package body Q is private end Q;"),
        ("package instantiation positional", "package Q is new R (1, 2);"),
        ("package instantiation box", "package Q is new R (<>);"),
        ("package instantiation formal subprogram", "package Q is new R (Func => F);"),
        ("package instantiation operator", "package Q is new R (\"+\");"),
        ("package instantiation selected", "package Q is new A.B.R (Integer);"),
        ("package instantiation no actuals", "package Q is new R;"),
        ("procedure instantiation no actuals", "procedure Q is new R;"),
        ("function instantiation operator", "function \"+\" is new R (Integer);"),
        ("procedure body empty decls", "procedure Q is begin null; end Q;"),
        ("procedure body end operator", "function \"+\" (A : T) return T is begin return A; end \"+\";"),
        ("procedure body end wrong", "procedure Q is begin null; end R;"),
        ("procedure body end none", "procedure Q is begin null; end;"),
        ("procedure body handler", "procedure Q is begin null; exception when others => null; end Q;"),
        ("procedure body only decl handler", "procedure Q is X : Integer; begin null; exception when others => null; end Q;"),
        ("procedure body dotted name", "procedure Q.R is begin null; end Q.R;"),
        ("procedure body renames aspect", "procedure Q renames R with Inline;"),
        ("procedure body in mode", "procedure Q (A : in Integer);"),
        ("procedure body out mode", "procedure Q (A : out Integer);"),
        ("procedure body in out mode", "procedure Q (A : in out Integer);"),
        ("procedure body access mode", "procedure Q (A : access Integer);"),
        ("procedure body all modes list", "procedure Q (A, B : in Integer; C : out Float; D : in out String);"),
        ("procedure body default", "procedure Q (A : Integer := 1);"),
        ("procedure body empty parens", "procedure Q ();"),
        ("procedure body trailing semicolon param", "procedure Q (A : Integer;);"),
        ("procedure body aspect before is", "procedure Q with Inline is begin null; end Q;"),
        ("function body aspect", "function Q return Integer with Pre => True is begin return 1; end Q;"),
        ("function body result class", "function Q return T'Class is begin return X; end Q;"),
        ("function body result selected", "function Q return A.B is begin return X; end Q;"),
        ("function body result not null", "function Q return not null access T is begin return X; end Q;"),
        ("function body result access constant", "function Q return access constant T is begin return X; end Q;"),
        ("function body result anon procedure", "function Q return access procedure is begin return X; end Q;"),
        ("function missing return type", "function Q return;"),
        ("function missing return", "function Q;"),
        ("function operator decl", "function \"<=\" (L, R : T) return Boolean;"),
        ("function operator bad", "function \"foo\" (L, R : T) return Boolean;"),
        ("function expression null", "function Q return T is (null);"),
        ("function expression aggregate", "function Q return T is (A => 1, B => 2);"),
        ("function expression conditional", "function Q (X : Integer) return Integer is (if X > 0 then X else 0) with Inline;"),
        ("function expression quantified", "function Q return Boolean is (for all I in 1 .. 3 => True);"),
        ("function abstract", "function Q return T is abstract;"),
        ("function separate", "function Q return T is separate;"),
        ("function null", "function Q return T is null;"),
        ("overriding abstract", "overriding function Q return T is abstract;"),
        ("overriding null", "overriding procedure Q is null;"),
        ("overriding expression", "overriding function Q return T is (X);"),
        ("overriding renames", "overriding procedure Q renames R;"),
        ("overriding instantiation", "overriding procedure Q is new R (Integer);"),
        ("not overriding function", "not overriding function Q return T is begin return X; end Q;"),
        ("not overriding abstract", "not overriding procedure Q is abstract;"),
        ("overriding twice", "overriding overriding procedure Q is null;"),
        ("not alone", "not procedure Q is null;"),
        ("use package", "use A.B;"),
        ("use list", "use A, B;"),
        ("use type list", "use type A, B;"),
        ("use all type list", "use all type A.B, C;"),
        ("use string", "use \"+\";"),
        ("use attribute", "use A'Class;"),
        ("use empty", "use ;"),
        ("pragma no args", "pragma P;"),
        ("pragma one arg", "pragma P (A);"),
        ("pragma named", "pragma P (A => B, C => D);"),
        ("pragma expression", "pragma P (A + 1, \"x\");"),
        ("pragma conditional", "pragma P (if A then B else C);"),
        ("pragma attribute", "pragma P (A'Access);"),
        ("pragma reserved name", "pragma Interface (C, Q);"),
        ("pragma empty parens", "pragma P ();"),
        ("pragma missing semicolon", "pragma P"),
        ("pragma in aggregate", "X : T := (pragma P;);"),
        ("pragma ada_83 inside generic like", "procedure Q is begin pragma Ada_83; null; end Q;"),
        ("pragma edition after decl", "X : Integer;\npragma Ada_83;"),
        ("pragma edition mixed case", "pragma aDa_83;"),
        ("pragma extensions case", "pragma EXTENSIONS_ALLOWED (On);"),
        ("pragma edition similar", "pragma Ada_83x;"),
        ("pragma edition prefix", "pragma Ada_8;"),
        ("pragma edition as argument", "pragma P (Ada_83);"),
        ("pragma edition in string", "pragma P (\"Ada_83\");"),
        ("pragma edition comment", "-- pragma Ada_83;\nnull;"),
    ] {
        add(&format!("declaration: {name}"), decl(declaration));
        add(&format!("declaration local: {name}"), local_decl(declaration));
    }

    // ---- statements -------------------------------------------------------
    for (name, statement) in [
        ("if no then", "if A null; end if;"),
        ("if elsif no then", "if A then null; elsif B null; end if;"),
        ("if else twice", "if A then null; else null; else null; end if;"),
        ("if elsif after else", "if A then null; else null; elsif B then null; end if;"),
        ("if missing end if", "if A then null; end;"),
        ("if end loop", "if A then null; end loop;"),
        ("if empty branch", "if A then else null; end if;"),
        ("if empty then", "if A then end if;"),
        ("if pragma only", "if A then pragma P; end if;"),
        ("if nested", "if A then if B then null; end if; end if;"),
        ("if label", "L : if A then null; end if;"),
        ("if label end", "L : if A then null; end if L;"),
        ("case empty when", "case X is when 1 => end case;"),
        ("case empty", "case X is end case;"),
        ("case others middle", "case X is when others => null; when 1 => null; end case;"),
        ("case others list", "case X is when 1 | others => null; end case;"),
        ("case expr statement", "case F (1) is when 1 => null; when others => null; end case;"),
        ("case choice qualified", "case X is when T'(1) => null; when others => null; end case;"),
        ("case choice subtype range", "case X is when A range 1 .. 2 => null; when others => null; end case;"),
        ("case choice attribute range", "case X is when A'Range => null; when others => null; end case;"),
        ("case choice expression", "case X is when 1 + 2 => null; when others => null; end case;"),
        ("case choice string", "case X is when \"a\" => null; when others => null; end case;"),
        ("case choice char", "case X is when 'a' .. 'z' => null; when others => null; end case;"),
        ("case missing arrow", "case X is when 1 null; end case;"),
        ("case no is", "case X when 1 => null; end case;"),
        ("case pragma between", "case X is pragma P; when 1 => null; end case;"),
        ("case label", "L : case X is when others => null; end case;"),
        ("loop plain", "loop null; end loop;"),
        ("loop label end", "L : loop null; end loop L;"),
        ("loop label mismatch", "L : loop null; end loop M;"),
        ("loop while", "while A and B loop null; end loop;"),
        ("loop while label", "L : while A loop null; end loop L;"),
        ("loop for range", "for I in A .. B loop null; end loop;"),
        ("loop for subtype range", "for I in T range A .. B loop null; end loop;"),
        ("loop for subtype", "for I in T loop null; end loop;"),
        ("loop for attribute", "for I in A'Range loop null; end loop;"),
        ("loop for reverse subtype", "for I in reverse T loop null; end loop;"),
        ("loop for of", "for E of A loop null; end loop;"),
        ("loop for of reverse", "for E of reverse A loop null; end loop;"),
        ("loop for of call", "for E of F (1) loop null; end loop;"),
        ("loop for in call", "for C in F (1) loop null; end loop;"),
        ("loop for in iterator", "for C in A.Iterate loop null; end loop;"),
        ("loop for typed", "for I : Integer in 1 .. 3 loop null; end loop;"),
        ("loop for typed of", "for E : Integer of A loop null; end loop;"),
        ("loop for filter", "for I in 1 .. 3 when I > 1 loop null; end loop;"),
        ("loop for parallel", "for I in parallel 1 .. 3 loop null; end loop;"),
        ("loop for no loop", "for I in 1 .. 3 null; end loop;"),
        ("loop for no in", "for I 1 .. 3 loop null; end loop;"),
        ("loop for expression bound", "for I in A + 1 .. B * 2 loop null; end loop;"),
        ("loop for call bound", "for I in F (1) .. G (2) loop null; end loop;"),
        ("loop for empty", "for I in 1 .. 3 loop end loop;"),
        ("loop exit when label", "L : loop exit L when A; end loop L;"),
        ("exit selected", "loop exit A.B; end loop;"),
        ("exit when only", "loop exit when A; end loop;"),
        ("exit bare", "loop exit; end loop;"),
        ("exit when missing expr", "loop exit when; end loop;"),
        ("block plain", "begin null; end;"),
        ("block declare", "declare X : Integer; begin null; end;"),
        ("block declare empty", "declare begin null; end;"),
        ("block label", "B : begin null; end B;"),
        ("block label declare", "B : declare X : Integer; begin null; end B;"),
        ("block label mismatch", "B : begin null; end C;"),
        ("block no begin", "declare X : Integer; null; end;"),
        ("block handler pragma", "begin null; exception pragma P; when others => null; end;"),
        ("block handler empty", "begin null; exception when others => end;"),
        ("block handler no when", "begin null; exception null; end;"),
        ("block handler choice selected", "begin null; exception when A.B => null; end;"),
        ("block handler choice list", "begin null; exception when A | B | others => null; end;"),
        ("block handler named", "begin null; exception when E : A | B => null; end;"),
        ("block handler named others", "begin null; exception when E : others => null; end;"),
        ("block handler nested", "begin begin null; exception when others => null; end; exception when others => null; end;"),
        ("block handler in handler", "begin null; exception when others => begin null; exception when others => null; end; end;"),
        ("block handler reraise", "begin null; exception when others => raise; end;"),
        ("block handler raise with", "begin null; exception when E : others => raise Program_Error with \"x\"; end;"),
        ("raise bare", "raise;"),
        ("raise name", "raise A;"),
        ("raise selected", "raise A.B.C;"),
        ("raise with", "raise A with \"m\";"),
        ("raise with expr", "raise A with \"m\" & Image (1);"),
        ("raise with only", "raise with \"m\";"),
        ("raise call", "raise F (1);"),
        ("raise string", "raise \"A\";"),
        ("return plain", "return;"),
        ("return expr", "return A + 1;"),
        ("return aggregate", "return (A => 1);"),
        ("return null", "return null;"),
        ("return if", "return (if A then 1 else 2);"),
        ("return extended", "return R : Integer := 1 do null; end return;"),
        ("return extended no do", "return R : Integer := 1;"),
        ("return extended aliased", "return R : aliased Integer;"),
        ("return extended constant", "return R : constant Integer := 1;"),
        ("return extended access", "return R : access Integer := null;"),
        ("return extended handler", "return R : Integer do null; exception when others => null; end return;"),
        ("return extended end name", "return R : Integer do null; end return R;"),
        ("return extended empty do", "return R : Integer do end return;"),
        ("goto label", "goto A; <<A>> null;"),
        ("goto selected", "goto A.B;"),
        ("label only", "<<A>>"),
        ("label ends block", "begin <<A>> end;"),
        ("label stacked", "<<A>> <<B>> <<C>> null;"),
        ("label before declare", "<<A>> declare begin null; end;"),
        ("label before if", "<<A>> if B then null; end if;"),
        ("label reserved", "<<end>> null;"),
        ("label not identifier", "<<1>> null;"),
        ("label unclosed", "<<A null;"),
        ("call plain", "P;"),
        ("call attribute", "A'Output (S, X);"),
        ("call attribute no args", "A'Read;"),
        ("call attribute chain", "A.B'Write (S, X);"),
        ("call qualified", "T'(1);"),
        ("call string", "\"x\";"),
        ("call operator", "\"+\" (1, 2);"),
        ("call literal", "1;"),
        ("call char", "'x';"),
        ("call parenthesised", "(P);"),
        ("call aggregate", "(1, 2);"),
        ("call null", "null (1);"),
        ("call new", "new T;"),
        ("call deref", "P.all;"),
        ("call deref args", "P.all (1);"),
        ("call deref chain", "P.all.Q (1);"),
        ("call index chain", "A (1).B (2);"),
        ("call named only", "P (A => 1);"),
        ("call positional after named", "P (A => 1, 2);"),
        ("call box", "P (<>);"),
        ("call empty", "P ();"),
        ("call trailing comma", "P (1,);"),
        ("call leading comma", "P (, 1);"),
        ("call unmatched open", "P (1;"),
        ("call unmatched close", "P 1);"),
        ("call nested unmatched", "P (Q (1);"),
        ("assignment plain", "X := 1;"),
        ("assignment aggregate target", "(A, B) := C;"),
        ("assignment attribute target", "A'First := 1;"),
        ("assignment call target", "F (1) := 2;"),
        ("assignment deref target", "P.all := 2;"),
        ("assignment selected target", "A.B.C := 2;"),
        ("assignment chain", "A := B := C;"),
        ("assignment equal", "A = B;"),
        ("assignment missing rhs", "A := ;"),
        ("assignment missing lhs", ":= 1;"),
        ("assignment colon equal spaced", "A : = 1;"),
        ("assignment string target", "\"x\" := 1;"),
        ("assignment qualified rhs", "A := T'(1);"),
        ("assignment target name", "A := @ + 1;"),
        ("assignment raise rhs", "A := raise Error;"),
        ("assignment if rhs", "A := (if B then 1 else 2);"),
        ("assignment case rhs", "A := (case B is when others => 1);"),
        ("assignment declare rhs", "A := (declare X : Integer := 1; begin X);"),
        ("pragma statement ada83", "pragma Ada_83;"),
        ("pragma statement unknown", "pragma Whatever (A, B => C);"),
        ("pragma statement assert", "pragma Assert (A, \"m\");"),
        ("pragma statement between", "null; pragma P; null;"),
        ("pragma statement only", "pragma P;"),
        ("null null", "null; null;"),
        ("null missing semicolon", "null"),
        ("null extra semicolons", "null;;"),
        ("empty statement", ";"),
        ("semicolons only", ";;;"),
        ("accept", "accept A;"),
        ("delay", "delay 1.0;"),
        ("abort", "abort A;"),
        ("requeue", "requeue A;"),
        ("select", "select null; or null; end select;"),
        ("entry call", "A.B (1);"),
        ("parallel block", "parallel do null; and null; end do;"),
        ("pragma Ada_2022", "pragma Ada_2022;"),
    ] {
        add(&format!("stress statement: {name}"), body(&format!("{statement}\n")));
        add(
            &format!("stress statement nested: {name}"),
            body(&format!("begin\n{statement}\nend;\n")),
        );
    }

    // ---- refusal precedence ----------------------------------------------
    let deep = format!("X := {}1{};\n", "(".repeat(100), ")".repeat(100));
    for (name, prefix, suffix) in [
        ("pragma then deep", "pragma Ada_83;\n", ""),
        ("deep then pragma", "", "pragma Ada_83;\n"),
        ("deep then outside", "", "task T;\n"),
        ("outside then deep", "accept A;\n", ""),
        ("deep then syntax error", "", "X := ;\n"),
        ("syntax error then deep", "X := ;\n", ""),
        ("deep then trailing unit", "", ""),
    ] {
        add(
            &format!("precedence: {name}"),
            body(&format!("{prefix}{deep}{suffix}")),
        );
        add(
            &format!("precedence decl: {name}"),
            decl(&format!(
                "{prefix}{}",
                deep.replace("X := ", "X : T := ").replace(";\n", ";")
            )),
        );
    }
    add(
        "precedence: deep then second unit",
        format!("{}{}", body(&deep), body(REG)),
    );
    add(
        "precedence: pragma in task",
        decl("task body T is begin pragma Ada_83; null; end T;"),
    );
    add(
        "precedence: pragma after generic",
        decl("generic type T is private; procedure P (X : T);\npragma Ada_83;"),
    );
    add(
        "precedence: pragma then library spec",
        "pragma Ada_83;\npackage Q is end Q;\n".to_string(),
    );
    add(
        "precedence: pragma then subunit",
        "pragma Ada_83;\nseparate (P)\nprocedure Q is begin null; end Q;\n".to_string(),
    );
    add(
        "precedence: library spec then pragma",
        "package Q is pragma Ada_83; end Q;\n".to_string(),
    );
    add(
        "precedence: with then pragma then spec",
        "with A;\npragma Ada_83;\nprocedure Q;\n".to_string(),
    );
    add(
        "precedence: pragma in second statement of missing end",
        "package body Q is\nprocedure R is begin null; pragma Ada_83; end R;\n".to_string(),
    );
    add(
        "precedence: edition word beats pragma",
        "pragma Ada_83;\npackage body Q is\nInterface : Integer;\nend Q;\n".to_string(),
    );
    add(
        "precedence: lexical beats pragma",
        "pragma Ada_83;\npackage body Q is\nX : Integer := 1 @ 2;\nend Q;\n".to_string(),
    );
    add(
        "precedence: pragma inside deep parens",
        body(&format!(
            "X := {}pragma Ada_83;{};\n",
            "(".repeat(100),
            ")".repeat(100)
        )),
    );
    add(
        "precedence: pragma before eof garbage",
        "pragma Ada_83;\n)))".to_string(),
    );
    add(
        "precedence: garbage before pragma",
        ")))\npragma Ada_83;\n".to_string(),
    );
    corpus
}

type DepthShape = Box<dyn Fn(usize) -> String>;

/// Prints the first nesting level at which each frontend refuses, per shape.
#[test]
#[ignore]
fn probe_depth_thresholds() {
    let shapes: Vec<(&str, DepthShape)> = vec![
        (
            "conditionals",
            Box::new(|l| {
                decl(&format!(
                    "X : Integer := {}1{};",
                    "(if A then ".repeat(l),
                    " else 2)".repeat(l)
                ))
            }),
        ),
        (
            "qualified",
            Box::new(|l| body(&format!("X := {}1{};\n", "T'(".repeat(l), ")".repeat(l)))),
        ),
        (
            "aggregates",
            Box::new(|l| {
                decl(&format!(
                    "X : Arr := {}1{};",
                    "(1, ".repeat(l),
                    ")".repeat(l)
                ))
            }),
        ),
        (
            "indexing",
            Box::new(|l| body(&format!("X := T {}1{};\n", "(T ".repeat(l), ")".repeat(l)))),
        ),
    ];
    for (name, make) in shapes {
        let first = |parse: fn(&str) -> Result<Result<AdmittedUnit, Refusal>, ParseError>| {
            (1..120).find(|l| parse(&make(*l)).unwrap() == Err(Refusal::NestingLimit))
        };
        println!(
            "{name}: legacy {:?} tree {:?}",
            first(legacy_parse),
            first(syntax::parse_compilation)
        );
    }
}

#[test]
fn legacy_and_tree_sitter_agree_on_every_committed_fixture() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/fixtures/ada");
    let mut compared = 0;
    let mut stack = vec![root];
    while let Some(directory) = stack.pop() {
        for entry in std::fs::read_dir(&directory).expect("fixture directory") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|extension| extension == "adb") {
                let text = std::fs::read_to_string(&path).expect("fixture text");
                if let Some(difference) = difference(&text) {
                    panic!("{}: {difference}", path.display());
                }
                compared += 1;
            }
        }
    }
    assert!(
        compared >= 3,
        "expected the committed Ada fixtures, found {compared}"
    );
}

/// Branch-corpus sources on which the two frontends differ, matched by name
/// prefix, each with its preregistered class and the root cause.
/// `unknown_added`: the legacy parser accepted text that is not valid Ada (or
/// uses a construct newer than the grammar) and the grammar refuses it.
/// `unknown_removed`: valid Ada, or a construct the grammar reads, that the
/// legacy parser could not.
const CLASSIFIED: &[(&str, &str, &str)] = &[
    // ---- unknown_added: the legacy parser was more lenient than Ada --------
    ("ordinary: subtype not null", "unknown_added", "subtype indication took `not null access`, which only a type declaration allows"),
    ("ordinary local: subtype not null", "unknown_added", "same"),
    ("case without alternatives", "unknown_added", "a case statement needs one alternative"),
    ("stress statement: case empty", "unknown_added", "a case statement needs one alternative"),
    ("stress statement nested: case empty", "unknown_added", "same"),
    ("ordinary: and then", "unknown_added", "`and then` mixed with `or else` needs parentheses"),
    ("ordinary local: and then", "unknown_added", "same"),
    ("statement: if compound", "unknown_added", "same"),
    ("expression: and or mix", "unknown_added", "`and` mixed with `or` needs parentheses"),
    ("expression: and then or else", "unknown_added", "same"),
    ("expression: or xor", "unknown_added", "same"),
    ("expression: and xor", "unknown_added", "same"),
    ("anchor: range second", "unknown_added", "a range is not an actual parameter"),
    ("anchor no with clause: range second", "unknown_added", "same"),
    ("anchor: choice second", "unknown_added", "a choice list is not an actual parameter"),
    ("anchor no with clause: choice second", "unknown_added", "same"),
    ("expression: new access", "unknown_added", "an allocator names a subtype, not an access definition"),
    ("expression: attribute in", "unknown_added", "`'Range` is a range, not an expression"),
    ("expression: range attr expr", "unknown_added", "same"),
    ("word attribute range", "unknown_added", "same"),
    ("word attribute arg range", "unknown_added", "same"),
    ("word named association null", "unknown_added", "`null` is not a formal parameter name"),
    ("word handler null", "unknown_added", "`null` is not an exception choice"),
    ("declaration: subtype range box", "unknown_added", "`range <>` is a formal-type form"),
    ("declaration local: subtype range box", "unknown_added", "same"),
    ("declaration: subtype access", "unknown_added", "a subtype of an anonymous access definition"),
    ("declaration local: subtype access", "unknown_added", "same"),
    ("declaration: type range box", "unknown_added", "`range <>` is a formal-type form"),
    ("declaration local: type range box", "unknown_added", "same"),
    ("declaration: type record empty", "unknown_added", "a record needs a component or `null;`"),
    ("declaration local: type record empty", "unknown_added", "same"),
    ("declaration: type record null then field", "unknown_added", "`null;` cannot share a component list"),
    ("declaration local: type record null then field", "unknown_added", "same"),
    ("declaration: function null", "unknown_added", "only a procedure can be null"),
    ("declaration local: function null", "unknown_added", "same"),
    ("declaration: not alone", "unknown_added", "`not` needs `overriding`"),
    ("declaration local: not alone", "unknown_added", "same"),
    ("stress statement: if label", "unknown_added", "`L :` labels a loop or a block only"),
    ("stress statement nested: if label", "unknown_added", "same"),
    ("stress statement: case label", "unknown_added", "same"),
    ("stress statement nested: case label", "unknown_added", "same"),
    ("statement: for typed", "unknown_added", "Ada 2022 `for I : T in`, which the pinned grammar predates"),
    ("stress statement: loop for typed", "unknown_added", "same"),
    ("stress statement nested: loop for typed", "unknown_added", "same"),
    // ---- unknown_removed: the legacy parser lacked the production ----------
    ("ordinary: generic instantiation function", "unknown_removed", "the legacy function specification demanded `return`, so a function instantiation was refused"),
    ("ordinary local: generic instantiation function", "unknown_removed", "same"),
    ("declaration: function instantiation operator", "unknown_removed", "same"),
    ("declaration local: function instantiation operator", "unknown_removed", "same"),
    ("ordinary: aggregate range", "unknown_removed", "the legacy association parser did not read `=>` after a range choice"),
    ("ordinary local: aggregate range", "unknown_removed", "same"),
    ("expression: aggregate range", "unknown_removed", "same"),
    ("expression: quantified typed", "unknown_removed", "Ada 2022 typed quantifier, not read by the legacy parser"),
    ("declaration: interface list derived", "unknown_removed", "the legacy derived-type parser did not read an interface list (`and I`)"),
    ("declaration local: interface list derived", "unknown_removed", "same"),
    ("declaration: derived with private interface", "unknown_removed", "same"),
    ("declaration local: derived with private interface", "unknown_removed", "same"),
    ("declaration: object no type renames", "unknown_removed", "Ada 2022 `X renames Y;`, not read by the legacy parser"),
    ("declaration local: object no type renames", "unknown_removed", "same"),
    ("declaration: type incomplete tagged", "unknown_removed", "Ada 2005 `type T is tagged;`, not read by the legacy parser"),
    ("declaration local: type incomplete tagged", "unknown_removed", "same"),
    ("declaration: type record pragma", "unknown_removed", "the legacy component list did not read pragmas"),
    ("declaration local: type record pragma", "unknown_removed", "same"),
    ("declaration: type record end name", "unknown_removed", "the grammar reads a name after `end record`"),
    ("declaration local: type record end name", "unknown_removed", "same"),
    ("stress statement: loop for filter", "unknown_removed", "Ada 2022 iterator filter, not read by the legacy parser"),
    ("stress statement nested: loop for filter", "unknown_removed", "same"),
];

#[test]
fn legacy_and_tree_sitter_agree_on_the_branch_corpus() {
    let corpus = branch_corpus();
    let mut unexpected = Vec::new();
    let mut seen: Vec<usize> = vec![0; CLASSIFIED.len()];
    for (name, source) in &corpus {
        let Some(difference) = difference(source) else {
            continue;
        };
        let old = legacy_syntax::parse_compilation(source);
        let new = syntax::parse_compilation(source).expect("grammar loads");
        let known = CLASSIFIED
            .iter()
            .position(|(prefix, _, _)| name.starts_with(prefix));
        match known.map(|index| (index, CLASSIFIED[index].1)) {
            Some((index, "unknown_added"))
                if old.is_ok() && new == Err(Refusal::OutsideDeclaredSubset) =>
            {
                seen[index] += 1;
            }
            Some((index, "unknown_removed"))
                if old == Err(Refusal::OutsideDeclaredSubset) && new.is_ok() =>
            {
                seen[index] += 1;
            }
            _ => unexpected.push(format!("{name}: {difference}")),
        }
    }
    let added: usize = CLASSIFIED
        .iter()
        .zip(&seen)
        .filter(|((_, class, _), _)| *class == "unknown_added")
        .map(|(_, count)| count)
        .sum();
    let removed: usize = CLASSIFIED
        .iter()
        .zip(&seen)
        .filter(|((_, class, _), _)| *class == "unknown_removed")
        .map(|(_, count)| count)
        .sum();
    println!(
        "branch corpus: {} sources, {added} unknown_added, {removed} unknown_removed",
        corpus.len()
    );
    assert!(
        unexpected.is_empty(),
        "{} of {} branch sources differ unexpectedly:\n{}",
        unexpected.len(),
        corpus.len(),
        unexpected.join("\n")
    );
    for ((prefix, _, _), count) in CLASSIFIED.iter().zip(&seen) {
        assert!(
            *count > 0,
            "classified difference no longer differs: {prefix}"
        );
    }
}

/// Compares the two frontends over `REPOGRAMMAR_ADA_DIFFERENTIAL_CORPUS`, a
/// directory of `.adb`/`.ads` files, without failing on differences.
#[test]
#[ignore]
fn external_corpus_differential() {
    let Ok(root) = std::env::var("REPOGRAMMAR_ADA_DIFFERENTIAL_CORPUS") else {
        println!("REPOGRAMMAR_ADA_DIFFERENTIAL_CORPUS is not set");
        return;
    };
    let mut files = Vec::new();
    let mut stack = vec![std::path::PathBuf::from(root)];
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
                .is_some_and(|extension| extension == "adb" || extension == "ads")
            {
                files.push(path);
            }
        }
    }
    files.sort();
    let (mut compared, mut differing, mut legacy_panics) = (0, 0, 0);
    for path in &files {
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        compared += 1;
        let legacy = std::panic::catch_unwind(|| legacy_syntax::parse_compilation(&text));
        let new = syntax::parse_compilation(&text).expect("grammar loads");
        match legacy {
            Err(_) => {
                legacy_panics += 1;
                println!("PANIC legacy {} (tree-sitter: {new:?})", path.display());
            }
            Ok(old) if old != new => {
                differing += 1;
                println!(
                    "DIFF {}: legacy {old:?} vs tree-sitter {new:?}",
                    path.display()
                );
            }
            Ok(old) => println!("SAME {}: {old:?}", path.display()),
        }
    }
    println!(
        "external corpus: {compared} compared, {differing} differing, {legacy_panics} legacy panics"
    );
}
