#![cfg(unix)]

use super::project_session::PythonProjectSession;
use super::python_runtime_qualification_tests::versioned_executable;
use super::*;
use crate::adapters::filesystem::discovery::{sha256_hex, FilesystemFileDiscovery};
use crate::adapters::filesystem::source_store::FilesystemSourceStore;
use crate::adapters::frameworks::SyntaxFrameworkRoleDetector;
use crate::adapters::persistence::sqlite::SqliteIndexStore;
use crate::application::indexing::{
    index_repository_with_discovery_parser_frameworks_families_and_store, IndexingRequest,
};
use crate::core::model::ContentHash;
use crate::ports::family_store::FamilyStore;
use crate::ports::index_store::IndexStore;
use crate::test_support::TempWorkspace;

fn source<'a>(path: &'a str, text: &'a str) -> SourceDocument<'a> {
    SourceDocument {
        path,
        text,
        language: Language::Python,
        content_hash: ContentHash::new(format!("sha256:{}", sha256_hex(text.as_bytes()))).unwrap(),
        repository_revision: RepositoryRevision::new("UNKNOWN").unwrap(),
    }
}

#[test]
fn qualified_default_and_explicit_legacy_selection_are_strict() {
    assert_eq!(project_session_selected(None), Ok(true));
    for (value, expected) in [("1", true), ("0", false)] {
        assert_eq!(
            project_session_selected(Some(std::ffi::OsStr::new(value))),
            Ok(expected)
        );
    }
    for value in ["", "on", "off", "invalid"] {
        assert!(project_session_selected(Some(std::ffi::OsStr::new(value))).is_err());
    }
}

fn context(files: &[(&str, &str)]) -> ParserProjectContext {
    ParserProjectContext {
        python_module_paths: files.iter().map(|(path, _)| path.to_string()).collect(),
        python_module_files: files
            .iter()
            .map(
                |(path, text)| crate::ports::parser::ParserProjectFileContext {
                    path: path.to_string(),
                    text: text.to_string(),
                },
            )
            .collect(),
        python_conftest_files: files
            .iter()
            .filter(|(path, _)| path.ends_with("conftest.py"))
            .map(
                |(path, text)| crate::ports::parser::ParserProjectFileContext {
                    path: path.to_string(),
                    text: text.to_string(),
                },
            )
            .collect(),
        ..ParserProjectContext::default()
    }
}

#[test]
fn session_matches_legacy_document_values_and_context_omission() {
    let files = [
        ("pkg/service.py", "class Service: pass\n__all__ = ['Service']\n"),
        ("pkg/__init__.py", "from .service import Service as Exported\n"),
        ("tests/conftest.py", "import pytest\n@pytest.fixture(name='client')\ndef make_client(): return object()\n"),
        ("tests/sub/conftest.py", "import pytest\n@pytest.fixture\ndef client(): return None\n"),
        ("tests/sub/test_api.py", "from pkg import Exported\nfrom pkg.service import *\nfrom missing import value\ndef test_api(client, tmp_path, unknown_fixture):\n    return Exported()\n"),
        ("broken.py", "def broken(:\n"),
        ("dynamic.py", "import importlib\nvalue = importlib.import_module(name)\n"),
    ];
    let parser = PythonAstParser::default();
    for oversized in [false, true] {
        let mut project = context(&files);
        if oversized {
            project
                .python_module_files
                .push(crate::ports::parser::ParserProjectFileContext {
                    path: "large.py".into(),
                    text: format!("#{}\n", "x".repeat(MAX_PYTHON_FRONTEND_INPUT_BYTES)),
                });
        }
        let mut session = PythonProjectSession::start(&parser, &project).expect("start session");
        for (path, text) in files {
            let old = parser
                .parse_with_context_output(source(path, text), &project)
                .expect("legacy parse");
            let new = session.parse(source(path, text)).expect("session parse");
            assert_eq!(old, new, "{path}, context omitted={oversized}");
        }
        session.finish().expect("EOS and exit");
    }
}

fn fixture_python_files(
    root: &std::path::Path,
    directory: &std::path::Path,
    files: &mut Vec<(String, String)>,
) {
    let mut entries = fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            fixture_python_files(root, &path, files);
        } else if path.extension().is_some_and(|extension| extension == "py") {
            files.push((
                path.strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/"),
                fs::read_to_string(&path).unwrap(),
            ));
        }
    }
}

#[test]
fn every_python_release_fixture_has_canonical_session_equivalence() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/fixtures/python/release/v0_1");
    let parser = PythonAstParser::default();
    let mut checked = 0;
    for fixture in fs::read_dir(&root).unwrap() {
        let directory = fixture.unwrap().path();
        if !directory.is_dir() {
            continue;
        }
        let mut files = Vec::new();
        fixture_python_files(&directory, &directory, &mut files);
        if files.is_empty() {
            continue;
        }
        let borrowed = files
            .iter()
            .map(|(path, text)| (path.as_str(), text.as_str()))
            .collect::<Vec<_>>();
        let project = context(&borrowed);
        let mut session = PythonProjectSession::start(&parser, &project).unwrap();
        for (path, text) in &files {
            assert_eq!(
                parser
                    .parse_with_context_output(source(path, text), &project)
                    .unwrap(),
                session.parse(source(path, text)).unwrap(),
                "{} / {path}",
                directory.display()
            );
            checked += 1;
        }
        session.finish().unwrap();
    }
    assert!(
        checked >= 20,
        "release corpus unexpectedly empty or incomplete: {checked}"
    );
}

#[test]
fn source_roots_and_ordered_reexport_collisions_match_legacy() {
    let files = [
        ("src/pkg/a.py", "class A: pass\n__all__ = ['A']\n"),
        ("src/pkg/z.py", "class A: pass\n"),
        (
            "src/pkg/__init__.py",
            "from .a import A\nfrom .z import A\n",
        ),
        ("app/pkg/a.py", "def A(): return 1\n"),
        ("consumer.py", "from pkg import A\nfrom pkg.a import *\n"),
    ];
    let mut project = context(&files);
    project.python_source_roots = vec!["src".into(), "app".into()];
    let parser = PythonAstParser::default();
    let mut session = PythonProjectSession::start(&parser, &project).unwrap();
    for (path, text) in files {
        assert_eq!(
            parser
                .parse_with_context_output(source(path, text), &project)
                .unwrap(),
            session.parse(source(path, text)).unwrap()
        );
    }
    session.finish().unwrap();
}

#[test]
fn session_omission_decision_matches_exact_serialized_json_length() {
    let text = "def visible(): return None\n";
    let doc = source("a.py", text);
    // Unicode, escaping and strings at the cap must use encoded bytes, not
    // character counts or a permissive estimated-size comparison.
    for padding in [
        0,
        100,
        MAX_PYTHON_FRONTEND_INPUT_BYTES - 400,
        MAX_PYTHON_FRONTEND_INPUT_BYTES,
    ] {
        let project = context(&[
            ("a.py", text),
            ("b.py", &format!("#é\\\"{}\n", "x".repeat(padding))),
        ]);
        let base = parse_document_payload(&doc, None).to_string().len();
        let added = python_project_context_payload(&project).to_string().len();
        let actual = parse_document_payload(&doc, Some(&project))
            .to_string()
            .len();
        assert_eq!(actual, base + added - 1);
        assert_eq!(
            serialize_parse_request(&doc, Some(&project)).unwrap().1,
            actual > MAX_PYTHON_FRONTEND_INPUT_BYTES
        );
    }
}

#[test]
fn session_preserves_legacy_newline_admission_at_the_exact_input_cap() {
    let parser = PythonAstParser::default();
    let project = ParserProjectContext::default();
    let base = parse_document_payload(&source("a.py", "#\n"), None)
        .to_string()
        .len();
    for requested_bytes in [
        MAX_PYTHON_FRONTEND_INPUT_BYTES - 1,
        MAX_PYTHON_FRONTEND_INPUT_BYTES,
        MAX_PYTHON_FRONTEND_INPUT_BYTES + 1,
    ] {
        let text = format!("#{}\n", "x".repeat(requested_bytes - base));
        assert_eq!(
            parse_document_payload(&source("a.py", &text), None)
                .to_string()
                .len(),
            requested_bytes
        );
        let legacy = parser.parse_with_context_output(source("a.py", &text), &project);
        let mut session = PythonProjectSession::start(&parser, &project).unwrap();
        let optimized = session.parse(source("a.py", &text));
        if requested_bytes < MAX_PYTHON_FRONTEND_INPUT_BYTES {
            assert_eq!(legacy.unwrap(), optimized.unwrap());
            session.finish().unwrap();
        } else {
            assert!(matches!(legacy, Err(ParseError::Internal(_))));
            assert!(matches!(optimized, Err(ParseError::Internal(_))));
        }
    }
}

#[test]
fn session_rejects_stale_content_hash_and_cannot_be_reused() {
    let mut session = PythonProjectSession::start(
        &PythonAstParser::default(),
        &ParserProjectContext::default(),
    )
    .unwrap();
    let mut stale = source("a.py", "def visible(): return 1\n");
    stale.content_hash = ContentHash::new(format!("sha256:{}", "0".repeat(64))).unwrap();
    assert!(matches!(
        session.parse(stale),
        Err(ParseError::PythonFrontendContractMismatch)
    ));
    assert!(session.parse(source("a.py", "pass\n")).is_err());
    assert!(session.finish().is_err());
}

struct SessionParser(PythonAstParser);
impl SourceParser for SessionParser {
    fn parse(&self, doc: SourceDocument<'_>) -> Result<ParseReport, ParseError> {
        self.0.parse(doc)
    }
    fn parse_with_context_output(
        &self,
        doc: SourceDocument<'_>,
        context: &ParserProjectContext,
    ) -> Result<SourceParseOutput, ParseError> {
        self.0.parse_with_context_output(doc, context)
    }
    fn begin_project_session(
        &self,
        context: &ParserProjectContext,
    ) -> Result<Option<Box<dyn ParserProjectSession>>, ParseError> {
        Ok(Some(Box::new(PythonProjectSession::start(
            &self.0, context,
        )?)))
    }
}

// Keep the old transport arm independent of the candidate's rollout default.
struct LegacyParser<'a>(&'a PythonAstParser);
impl SourceParser for LegacyParser<'_> {
    fn parse(&self, doc: SourceDocument<'_>) -> Result<ParseReport, ParseError> {
        self.0.parse(doc)
    }

    fn parse_with_context_output(
        &self,
        doc: SourceDocument<'_>,
        context: &ParserProjectContext,
    ) -> Result<SourceParseOutput, ParseError> {
        self.0.parse_with_context_output(doc, context)
    }
}

fn store(workspace: &TempWorkspace, name: &str, count: usize) -> (String, SqliteIndexStore) {
    let root = workspace.path().join(name);
    fs::create_dir_all(root.join(".repogrammar/tmp")).unwrap();
    fs::create_dir(root.join(".repogrammar/locks")).unwrap();
    for index in 0..count {
        fs::write(root.join(format!("api_{index}.py")), "from fastapi import APIRouter\nrouter = APIRouter()\n@router.get('/users')\ndef users(): return []\n").unwrap();
    }
    (
        root.to_string_lossy().into_owned(),
        SqliteIndexStore::new(root.join(".repogrammar")),
    )
}

#[test]
fn full_generation_session_has_one_worker_and_identical_owned_records() {
    for count in [8, 16] {
        let workspace = TempWorkspace::new("python-session-generation-equivalence");
        let executable = versioned_executable(&workspace, 3, 10);
        let parser =
            PythonAstParser::with_worker(executable, source_checkout_python_worker_script());
        let (legacy_root, legacy_store) = store(&workspace, "legacy", count);
        let (session_root, session_store) = store(&workspace, "session", count);
        index_repository_with_discovery_parser_frameworks_families_and_store(
            IndexingRequest::new(legacy_root),
            &FilesystemFileDiscovery,
            &FilesystemSourceStore,
            &LegacyParser(&parser),
            &SyntaxFrameworkRoleDetector,
            &legacy_store,
        )
        .unwrap();
        let launches = workspace.path().join("launches-3-10");
        assert_eq!(
            fs::read_to_string(&launches).unwrap().lines().count(),
            count
        );
        fs::write(&launches, "").unwrap();
        index_repository_with_discovery_parser_frameworks_families_and_store(
            IndexingRequest::new(session_root),
            &FilesystemFileDiscovery,
            &FilesystemSourceStore,
            &SessionParser(parser),
            &SyntaxFrameworkRoleDetector,
            &session_store,
        )
        .unwrap();
        assert_eq!(fs::read_to_string(&launches).unwrap().lines().count(), 1);
        assert_eq!(
            legacy_store.load_active_claim_input_snapshot().unwrap(),
            session_store.load_active_claim_input_snapshot().unwrap()
        );
        assert_eq!(
            legacy_store.list_active_families().unwrap(),
            session_store.list_active_families().unwrap()
        );
    }
}

fn fake_worker(workspace: &TempWorkspace, behavior: &str) -> PathBuf {
    let script = workspace.path().join("fake.py");
    let bundled =
        serde_json::to_string(&source_checkout_python_worker_script().to_string_lossy()).unwrap();
    fs::write(
        &script,
        format!(
            r#"import json, runpy, sys, time
api = runpy.run_path({bundled}, run_name='fixture_worker')
def emit(value):
    print(json.dumps(value, separators=(',', ':')), flush=True)
header = json.loads(sys.stdin.buffer.readline())
sys.stdin.buffer.readline()
emit(dict(header, message_type='ready'))
while True:
    header = json.loads(sys.stdin.buffer.readline())
    if header['message_type'] == 'finish':
        {behavior}
        break
    payload = json.loads(sys.stdin.buffer.readline())
    emit(dict(header, message_type='result'))
    emit(api['document_result'](payload, api['document_context']([], [], [], [])))
"#
        ),
    )
    .unwrap();
    script
}

#[test]
fn missing_or_false_eos_prevents_activation_of_partial_generation() {
    for behavior in ["sys.exit(0)", "emit(dict(header, message_type='end_of_stream')); print('{}', flush=True)", "emit(dict(header, message_type='end_of_stream')); sys.stdout.write('{'); sys.stdout.flush()", "emit(dict(header, message_type='end_of_stream')); sys.exit(9)"] {
        let workspace = TempWorkspace::new("python-session-rollback");
        let (root, store) = store(&workspace, "repository", 3);
        let admitted = PythonAstParser::default();
        index_repository_with_discovery_parser_frameworks_families_and_store(IndexingRequest::new(&root), &FilesystemFileDiscovery, &FilesystemSourceStore, &admitted, &SyntaxFrameworkRoleDetector, &store).unwrap();
        let before = store.load_active_claim_input_snapshot().unwrap();
        let malformed = SessionParser(PythonAstParser::with_worker(admitted.executable, fake_worker(&workspace, behavior)));
        assert!(index_repository_with_discovery_parser_frameworks_families_and_store(IndexingRequest::new(&root), &FilesystemFileDiscovery, &FilesystemSourceStore, &malformed, &SyntaxFrameworkRoleDetector, &store).is_err());
        assert_eq!(before, store.load_active_claim_input_snapshot().unwrap());
    }
}

#[test]
fn session_admits_runtime_and_rejects_bad_headers_crashes_and_timeout() {
    let workspace = TempWorkspace::new("python-session-rejection");
    let parser = PythonAstParser::with_worker(
        versioned_executable(&workspace, 3, 9),
        source_checkout_python_worker_script(),
    );
    assert!(matches!(
        PythonProjectSession::start(&parser, &ParserProjectContext::default()),
        Err(ParseError::PythonFrontendInterpreterUnsupported)
    ));
    for body in [
        "import sys\nsys.exit(9)\n",
        "print('{}', flush=True)\n",
        "print('x' * (2 * 1048576 + 1), flush=True)\n",
        "import time\ntime.sleep(10)\n",
    ] {
        let script = workspace.path().join("bad.py");
        fs::write(&script, body).unwrap();
        let parser = PythonAstParser::with_worker_timeout(
            PythonAstParser::default().executable,
            script,
            Duration::from_millis(150),
        );
        let start = Instant::now();
        assert!(PythonProjectSession::start(&parser, &ParserProjectContext::default()).is_err());
        assert!(start.elapsed() < Duration::from_secs(2));
    }
}

#[test]
fn parse_failures_and_shutdown_timeouts_are_bounded() {
    for behavior in [
        "time.sleep(10)",
        "sys.exit(9)",
        "print('x' * (2 * 1048576 + 1), flush=True); sys.exit(0)",
    ] {
        let workspace = TempWorkspace::new("python-session-parse-failure");
        let script = fake_worker(
            &workspace,
            "emit(dict(header, message_type='end_of_stream'))",
        );
        let original = fs::read_to_string(&script).unwrap();
        fs::write(
            &script,
            original.replace(
                "payload = json.loads(sys.stdin.buffer.readline())",
                &format!("{behavior}\n    payload = json.loads(sys.stdin.buffer.readline())"),
            ),
        )
        .unwrap();
        let parser = PythonAstParser::with_worker_timeout(
            PythonAstParser::default().executable,
            script,
            Duration::from_secs(5),
        );
        let mut session =
            PythonProjectSession::start(&parser, &ParserProjectContext::default()).unwrap();
        session.set_test_timeout(Duration::from_millis(300));
        let started = Instant::now();
        assert!(session.parse(source("a.py", "pass\n")).is_err());
        assert!(started.elapsed() < Duration::from_secs(2));
        assert!(session.finish().is_err());
    }
    let workspace = TempWorkspace::new("python-session-shutdown-timeout");
    let script = fake_worker(
        &workspace,
        "emit(dict(header, message_type='end_of_stream')); time.sleep(10)",
    );
    let parser = PythonAstParser::with_worker_timeout(
        PythonAstParser::default().executable,
        script,
        Duration::from_secs(5),
    );
    let mut session =
        PythonProjectSession::start(&parser, &ParserProjectContext::default()).unwrap();
    session.set_test_timeout(Duration::from_millis(300));
    let started = Instant::now();
    assert!(matches!(session.finish(), Err(ParseError::Timeout)));
    assert!(started.elapsed() < Duration::from_secs(2));
}
