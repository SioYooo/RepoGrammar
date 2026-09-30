#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use super::*;
use crate::adapters::filesystem::discovery::{sha256_hex, FilesystemFileDiscovery};
use crate::adapters::filesystem::source_store::FilesystemSourceStore;
use crate::adapters::frameworks::SyntaxFrameworkRoleDetector;
use crate::adapters::persistence::sqlite::SqliteIndexStore;
use crate::application::indexing::{
    index_repository_with_discovery_parser_frameworks_families_and_store,
    sync_repository_with_discovery_parser_frameworks_and_store, IndexingRequest,
};
use crate::core::model::ContentHash;
use crate::ports::family_store::FamilyStore;
use crate::ports::index_store::IndexStore;
use crate::test_support::TempWorkspace;

fn source<'a>(path: &'a str, language: Language, text: &'a str) -> SourceDocument<'a> {
    SourceDocument {
        path,
        language,
        text,
        content_hash: ContentHash::new(format!("sha256:{}", sha256_hex(text.as_bytes())))
            .expect("source hash"),
        repository_revision: RepositoryRevision::new("UNKNOWN").expect("revision"),
    }
}

fn shell_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\"'\"'"))
}

fn worker(workspace: &TempWorkspace, body: &str) -> PathBuf {
    let path = workspace.path().join("worker.py");
    fs::write(&path, body).expect("write fixture worker");
    path
}

// Execute the real bootstrap under a controlled version tuple. The wrapper
// handles the former direct-script launch too, so removing admission makes the
// rejection and non-dispatch assertions fail rather than skipping the test.
fn versioned_executable(workspace: &TempWorkspace, major: u32, minor: u32) -> String {
    let path = workspace.path().join(format!("python-{major}-{minor}"));
    let launches = workspace.path().join(format!("launches-{major}-{minor}"));
    let body = format!(
        r#"import collections, runpy, sys
with open({}, 'a') as trace: trace.write('launch\n')
version = collections.namedtuple('version_info', 'major minor micro releaselevel serial')
sys.version_info = version({major}, {minor}, 0, 'final', 0)
arguments = sys.argv[1:]
if arguments[0] == '-c':
    sys.argv = ['-c'] + arguments[2:]
    exec(compile(arguments[1], '<fixture-bootstrap>', 'exec'))
else:
    sys.argv = arguments
    runpy.run_path(arguments[0], run_name='__main__')
"#,
        serde_json::to_string(&launches.to_string_lossy()).expect("trace path")
    );
    let executable = PythonAstParser::default().executable;
    fs::write(
        &path,
        format!(
            "#!/bin/sh\nexec {} -c {} \"$@\"\n",
            shell_quote(&executable),
            shell_quote(&body)
        ),
    )
    .expect("write version wrapper");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).expect("make wrapper executable");
    path.to_string_lossy().into_owned()
}

fn marker_worker(workspace: &TempWorkspace) -> (PathBuf, PathBuf) {
    let marker = workspace.path().join("worker-dispatched");
    let body = format!(
        "from pathlib import Path\nPath({}).write_text('dispatched')\nprint('worker-result')\n",
        serde_json::to_string(&marker.to_string_lossy()).expect("marker path")
    );
    (worker(workspace, &body), marker)
}

#[test]
fn configured_real_interpreter_obeys_the_documented_runtime_boundary() {
    let parser = PythonAstParser::with_worker(
        PythonAstParser::default().executable,
        source_checkout_python_worker_script(),
    );
    let version = parser
        .python_frontend_version()
        .expect("real interpreter version");
    let mut parts = version
        .split('.')
        .map(|part| part.parse::<u32>().expect("numeric version"));
    let major = parts.next().expect("major version");
    let minor = parts.next().expect("minor version");
    let result = parser.parse(source(
        "app.py",
        Language::Python,
        "def visible():\n    return True\n",
    ));
    if major == 3 && minor >= 10 {
        assert_eq!(result.expect("real admitted runtime").units.len(), 2);
    } else {
        assert_eq!(
            result,
            Err(ParseError::PythonFrontendInterpreterUnsupported),
            "actual runtime {version}"
        );
    }
}

#[test]
fn unsupported_runtime_never_dispatches_document_config_or_interface_worker() {
    for (major, minor) in [(2, 7), (3, 9), (4, 0)] {
        let workspace = TempWorkspace::new("python-runtime-reject");
        let (script, marker) = marker_worker(&workspace);
        let parser =
            PythonAstParser::with_worker(versioned_executable(&workspace, major, minor), script);
        for (path, language, text) in [
            ("private-source.py", Language::Python, "secret_source = 1\n"),
            (
                "setup.cfg",
                Language::PythonConfig,
                "[metadata]\nname = private-name\n",
            ),
        ] {
            assert_eq!(
                parser.parse(source(path, language, text)),
                Err(ParseError::PythonFrontendInterpreterUnsupported),
                "runtime {major}.{minor}, {path}"
            );
        }
        assert_eq!(
            parser.extract_python_interface("private-source.py", "secret_source = 1\n"),
            PythonInterfaceProbe::Unverified
        );
        assert!(!marker.exists(), "unsupported worker was dispatched");
        assert_eq!(
            fs::read_to_string(workspace.path().join(format!("launches-{major}-{minor}")))
                .expect("launch trace")
                .lines()
                .count(),
            3,
            "one process per operation, with no separate version probe"
        );
    }
}

#[test]
fn minimum_runtime_dispatches_once_and_does_not_execute_source() {
    let workspace = TempWorkspace::new("python-runtime-accept");
    let parser = PythonAstParser::with_worker(
        versioned_executable(&workspace, 3, 10),
        source_checkout_python_worker_script(),
    );
    let sentinel = workspace.path().join("repository-code-executed");
    let text = format!(
        "from pathlib import Path\nPath({}).write_text('must-not-run')\ndef visible():\n    return True\n",
        serde_json::to_string(&sentinel.to_string_lossy()).expect("sentinel path")
    );
    let first = parser
        .parse_with_context_output(
            source("app.py", Language::Python, &text),
            &ParserProjectContext::default(),
        )
        .expect("minimum runtime parses supplied source");
    let second = parser
        .parse_with_context_output(
            source("app.py", Language::Python, &text),
            &ParserProjectContext::default(),
        )
        .expect("repeat identical parse");
    assert_eq!(first, second, "same input produces deterministic metadata");
    assert!(!first.report.units.is_empty());
    assert!(!sentinel.exists(), "repository source must not execute");
    assert_eq!(
        parser.extract_python_interface("app.py", &text),
        PythonInterfaceProbe::Computed(first.python_interface_hash.expect("interface hash"))
    );
    let config = parser
        .parse(source(
            "setup.cfg",
            Language::PythonConfig,
            "[metadata]\nname = example\n",
        ))
        .expect("minimum runtime project config");
    assert_eq!(config.units.len(), 1);
    assert_eq!(
        fs::read_to_string(workspace.path().join("launches-3-10"))
            .expect("launch trace")
            .lines()
            .count(),
        4
    );
}

#[test]
fn unsupported_runtime_wins_over_broken_pipe_for_a_bounded_large_request() {
    let workspace = TempWorkspace::new("python-runtime-early-exit");
    let (script, marker) = marker_worker(&workspace);
    let parser = PythonAstParser::with_worker(versioned_executable(&workspace, 3, 9), script);
    let text = format!("#{}\n", "private".repeat(100_000));
    assert_eq!(
        parser.parse(source("private.py", Language::Python, &text)),
        Err(ParseError::PythonFrontendInterpreterUnsupported)
    );
    assert!(!marker.exists());
}

#[test]
fn oversized_requests_are_refused_before_starting_an_interpreter() {
    let workspace = TempWorkspace::new("python-runtime-input-budget");
    let (script, marker) = marker_worker(&workspace);
    let parser = PythonAstParser::with_worker(versioned_executable(&workspace, 3, 10), script);
    let text = "x".repeat(MAX_PYTHON_FRONTEND_INPUT_BYTES + 1);
    for (path, language) in [
        ("app.py", Language::Python),
        ("setup.cfg", Language::PythonConfig),
    ] {
        assert_eq!(
            parser.parse(source(path, language, &text)),
            Err(ParseError::Internal(
                "python ast frontend request exceeded size limit".into()
            ))
        );
    }
    assert_eq!(
        parser.extract_python_interface("app.py", &text),
        PythonInterfaceProbe::Unverified
    );
    assert!(!workspace.path().join("launches-3-10").exists());
    assert!(!marker.exists());
}

#[test]
fn missing_runtime_and_worker_exit_code_lookalikes_are_not_version_evidence() {
    let workspace = TempWorkspace::new("python-runtime-lookalike");
    let absent = PythonAstParser::with_worker(
        workspace.path().join("absent-python").to_string_lossy(),
        source_checkout_python_worker_script(),
    );
    assert_eq!(
        absent.run_worker_request("{}", true),
        Err(ParseError::Internal(
            "python ast frontend is unavailable".into()
        ))
    );
    for body in [
        "import sys\nsys.stdin.read()\nsys.stderr.write('private-source /secret/path')\nsys.exit(78)\n",
        "import sys\nsys.stdin.read()\nprint('repogrammar-python-runtime-unsupported-extra')\nsys.exit(78)\n",
        "import sys\nsys.stdin.read()\nprint('repogrammar-python-runtime-unsupported')\nsys.exit(1)\n",
    ] {
        let parser = PythonAstParser::with_worker(
            versioned_executable(&workspace, 3, 10),
            worker(&workspace, body),
        );
        assert_eq!(
            parser.run_worker_request("{}", true),
            Err(ParseError::Internal("python ast frontend rejected parse request".into()))
        );
        assert_eq!(parser.extract_python_interface("app.py", "x=1\n"), PythonInterfaceProbe::Unverified);
    }
}

#[test]
fn admitted_runtime_preserves_malformed_oversized_non_utf8_and_timeout_failures() {
    let workspace = TempWorkspace::new("python-runtime-failures");
    let executable = versioned_executable(&workspace, 3, 10);
    for (body, expected) in [
        (
            "import sys\nsys.stdin.read()\nsys.stdout.buffer.write(b'\\xff')\n",
            "python ast frontend output was not UTF-8",
        ),
        (
            "import sys\nsys.stdin.read()\nsys.stdout.write('x' * (2 * 1024 * 1024 + 1))\n",
            "python ast frontend output exceeded size limit",
        ),
    ] {
        let parser = PythonAstParser::with_worker(&executable, worker(&workspace, body));
        assert_eq!(
            parser.run_worker_request("{}", true),
            Err(ParseError::Internal(expected.into()))
        );
        assert_eq!(
            parser.extract_python_interface("app.py", "x=1\n"),
            PythonInterfaceProbe::Unverified
        );
    }
    let malformed = PythonAstParser::with_worker(
        &executable,
        worker(
            &workspace,
            "import sys\nsys.stdin.read()\nprint('private-source malformed output')\n",
        ),
    );
    let error = malformed
        .parse(source(
            "private.py",
            Language::Python,
            "private_source = 1\n",
        ))
        .expect_err("malformed worker output rejected");
    assert!(matches!(error, ParseError::Internal(_)));
    assert!(!format!("{error:?}").contains("private"));
    assert_eq!(
        malformed.extract_python_interface("app.py", "x=1\n"),
        PythonInterfaceProbe::Unverified
    );

    let timeout = PythonAstParser::with_worker_timeout(
        executable,
        worker(
            &workspace,
            "import sys, time\nsys.stdin.read()\ntime.sleep(60)\n",
        ),
        Duration::from_millis(100),
    );
    let started = Instant::now();
    assert_eq!(
        timeout.run_worker_request("{}", true),
        Err(ParseError::Timeout)
    );
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "timeout must be bounded"
    );
    assert_eq!(
        timeout.extract_python_interface("app.py", "x=1\n"),
        PythonInterfaceProbe::Unverified
    );
}

#[test]
fn stale_interface_contract_is_unverified_on_an_admitted_runtime() {
    let workspace = TempWorkspace::new("python-runtime-stale-interface");
    let parser = PythonAstParser::with_worker(
        versioned_executable(&workspace, 3, 10),
        worker(&workspace, "import json, sys\nrequest=json.load(sys.stdin)\nrequest['contract_revision']=1\nrequest.pop('text')\nrequest['interface_hash']='sha256:'+'a'*64\nprint(json.dumps(request))\n"),
    );
    assert_eq!(
        parser.extract_python_interface("app.py", "x=1\n"),
        PythonInterfaceProbe::Unverified
    );
}

#[test]
fn bootstrap_ignores_repository_import_poison_and_preserves_worker_script_environment() {
    let workspace = TempWorkspace::new("python-runtime-import-boundary");
    let repository = workspace.path().join("repository");
    let trusted = workspace.path().join("trusted-worker");
    fs::create_dir(&repository).expect("repository");
    fs::create_dir(&trusted).expect("trusted worker directory");
    let poison = workspace.path().join("repository-runpy-executed");
    fs::write(
        repository.join("runpy.py"),
        format!(
            "from pathlib import Path\nPath({}).write_text('repository executed')\nraise RuntimeError('poison import')\n",
            serde_json::to_string(&poison.to_string_lossy()).expect("poison path")
        ),
    ).expect("poison repository module");
    fs::write(
        trusted.join("sibling.py"),
        "RESULT = 'trusted-worker-result'\n",
    )
    .expect("trusted sibling module");
    let script = trusted.join("worker.py");
    fs::write(&script, "import json, sibling, sys\nassert sys.argv == [__file__]\nassert json.load(sys.stdin) == {}\nprint(sibling.RESULT)\n")
        .expect("trusted worker");
    let executable = workspace.path().join("python-from-repository");
    fs::write(
        &executable,
        format!(
            "#!/bin/sh\ncd {} || exit 1\nexec {} -X frozen_modules=off \"$@\"\n",
            shell_quote(&repository.to_string_lossy()),
            shell_quote(&PythonAstParser::default().executable)
        ),
    )
    .expect("raw cwd wrapper");
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).expect("executable");
    // Recent Python freezes runpy, which would otherwise make an import-poison
    // test pass even with the guard removed. Prove the hostile fixture is live.
    let control = Command::new(&executable)
        .args(["-c", "import runpy"])
        .output()
        .expect("unguarded import control");
    assert!(!control.status.success());
    assert!(
        poison.exists(),
        "hostile fixture must be capable of executing"
    );
    fs::remove_file(&poison).expect("reset fixture sentinel");
    let parser = PythonAstParser::with_worker(executable.to_string_lossy(), script);
    assert_eq!(
        parser.run_worker_request("{}", true),
        Ok("trusted-worker-result\n".into())
    );
    assert!(
        !poison.exists(),
        "bootstrap must not import repository runpy.py"
    );
}

struct RuntimeChangedAfterInterfaceProbe<'a> {
    admitted: &'a PythonAstParser,
    unsupported: &'a PythonAstParser,
}

impl SourceParser for RuntimeChangedAfterInterfaceProbe<'_> {
    fn parse(&self, document: SourceDocument<'_>) -> Result<ParseReport, ParseError> {
        self.unsupported.parse(document)
    }

    fn extract_python_interface(&self, path: &str, text: &str) -> PythonInterfaceProbe {
        self.admitted.extract_python_interface(path, text)
    }
}

#[test]
fn unsupported_runtime_indexing_preserves_the_previous_active_generation() {
    let workspace = TempWorkspace::new("python-runtime-index-rollback");
    let repository = workspace.path().join("repository");
    fs::create_dir(&repository).expect("repository");
    fs::write(
        repository.join("app.py"),
        "def visible():\n    return True\n",
    )
    .expect("source");
    let state = repository.join(".repogrammar");
    fs::create_dir_all(state.join("tmp")).expect("state tmp");
    fs::create_dir(state.join("locks")).expect("state locks");
    let store = SqliteIndexStore::new(state);
    let accepted = PythonAstParser::with_worker(
        versioned_executable(&workspace, 3, 10),
        source_checkout_python_worker_script(),
    );
    let index = |parser: &PythonAstParser| {
        index_repository_with_discovery_parser_frameworks_families_and_store(
            IndexingRequest::new(repository.to_string_lossy()),
            &FilesystemFileDiscovery,
            &FilesystemSourceStore,
            parser,
            &SyntaxFrameworkRoleDetector,
            &store,
        )
    };
    index(&accepted).expect("initial admitted generation");
    let before_files = store.list_active_indexed_files().expect("active files");
    let before_units = store.list_active_code_units().expect("active units");
    let before_facts = store.list_active_semantic_facts().expect("active facts");
    let before_families = store.list_active_families().expect("active families");
    assert!(before_families.families.is_empty());
    let rejected = PythonAstParser::with_worker(
        versioned_executable(&workspace, 3, 9),
        source_checkout_python_worker_script(),
    );
    fs::write(
        repository.join("app.py"),
        "def visible():\n    return False\n",
    )
    .expect("body-only edit with stable interface");
    // The real admitted frontend supplies the interface hash, then the real
    // unsupported frontend handles parse. This models a runtime changed between
    // the two requests without mutating process-global environment or cwd.
    let changed_runtime = RuntimeChangedAfterInterfaceProbe {
        admitted: &accepted,
        unsupported: &rejected,
    };
    let error = sync_repository_with_discovery_parser_frameworks_and_store(
        IndexingRequest::new(repository.to_string_lossy()),
        &FilesystemFileDiscovery,
        &FilesystemSourceStore,
        &changed_runtime,
        &SyntaxFrameworkRoleDetector,
        &store,
    )
    .expect_err("incremental parse cannot reuse prior runtime admission");
    assert!(format!("{error:?}").contains("Python 3.10"));
    assert_eq!(
        store
            .list_active_indexed_files()
            .expect("preserved sync files"),
        before_files
    );
    assert_eq!(
        store
            .list_active_code_units()
            .expect("preserved sync units"),
        before_units
    );
    assert_eq!(
        store
            .list_active_semantic_facts()
            .expect("preserved sync facts"),
        before_facts
    );
    assert_eq!(
        store
            .list_active_families()
            .expect("preserved sync families"),
        before_families
    );
    fs::write(
        repository.join("test_runtime.py"),
        "def test_one(): pass\ndef test_two(): pass\ndef test_three(): pass\n",
    )
    .expect("new family-shaped source");
    let error = index(&rejected).expect_err("unsupported runtime cannot activate a generation");
    let displayed = format!("{error:?}");
    assert!(displayed.contains("Python 3.10"), "{displayed}");
    assert!(!displayed.contains(&repository.to_string_lossy().to_string()));
    assert!(!displayed.contains("def test"));
    assert_eq!(
        store.list_active_indexed_files().expect("preserved files"),
        before_files
    );
    assert_eq!(
        store.list_active_code_units().expect("preserved units"),
        before_units
    );
    assert_eq!(
        store.list_active_semantic_facts().expect("preserved facts"),
        before_facts
    );
    assert_eq!(
        store.list_active_families().expect("preserved families"),
        before_families
    );
    assert!(!repository.join(".repogrammar/locks/index.lock").exists());
    fs::write(
        repository.join("setup.cfg"),
        "[metadata]\nname = example\n[options]\npackage_dir =\n    = src\n",
    )
    .expect("root project config source-root preflight");
    let config_error =
        index(&rejected).expect_err("config preflight cannot admit unsupported runtime");
    assert_eq!(
        format!("{config_error:?}"),
        displayed,
        "one recovery across config and document routes"
    );
    assert_eq!(
        store
            .list_active_indexed_files()
            .expect("preserved config files"),
        before_files
    );
    assert_eq!(
        store
            .list_active_code_units()
            .expect("preserved config units"),
        before_units
    );
    assert_eq!(
        store
            .list_active_semantic_facts()
            .expect("preserved config facts"),
        before_facts
    );
    assert_eq!(
        store
            .list_active_families()
            .expect("preserved config families"),
        before_families
    );
    assert!(!repository.join(".repogrammar/locks/index.lock").exists());
    index(&accepted).expect("same fixture is actionable on an admitted runtime");
    assert!(!store
        .list_active_families()
        .expect("positive family control")
        .families
        .is_empty());
}
