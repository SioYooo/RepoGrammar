use crate::{
    adapters::persistence::sqlite::SqliteIndexStore,
    application::{
        query::{
            list_families, lookup_family, repo_shape_diagnostics, unknown_inventory,
            FamilyLookupMode, FamilyLookupReport,
        },
        repository::{
            RepositoryImplementationStatus, RepositoryManifestStatus, RepositoryReadiness,
            RepositoryStatus, RepositoryStatusReport, RepositoryStatusRequest,
        },
    },
    core::model::ContentHash,
    interfaces::{
        cli,
        mcp::{
            handle_context_call, McpOperation, McpReadOnlyRuntime, McpServeContext, McpToolName,
        },
    },
    ports::{
        family_store::{
            FamilyStore, IndexedFamilyEvidenceRecord, IndexedFamilyMemberRecord,
            IndexedFamilyRecord,
        },
        index_store::{
            GenerationHandle, IndexStore, IndexedCodeUnitRecord, IndexedFileRecord,
            IndexedSemanticFactRecord,
        },
    },
    test_support::TempWorkspace,
};
use serde_json::{json, Value};
use std::time::{Duration, Instant};

const READ_PATH_BENCH_CODE_UNITS: usize = 12_000;
const READ_PATH_BENCH_FAMILIES: usize = 300;
const READ_PATH_BENCH_EVIDENCE: usize = 25_000;
const READ_PATH_BENCH_SEMANTIC_FACTS: usize = 18_000;

#[test]
fn bootstrap_interfaces_are_reachable() {
    assert_eq!(cli::run(["--version"]).status, 0);
    assert_eq!(McpToolName::Context.as_str(), "repogrammar_context");
    assert_eq!(McpOperation::FindAnalogues.as_str(), "find_analogues");
}

#[test]
fn test_support_workspace_creates_real_temp_directory() {
    let workspace = TempWorkspace::new("integration");

    assert!(workspace.path().is_dir());
}

#[test]
#[ignore = "explicit read-path benchmark fixture; run with --ignored --nocapture"]
fn read_path_benchmark_fixture_measures_bounded_query_paths() {
    let fixture = build_read_path_benchmark_fixture();
    let runtime = BenchmarkMcpRuntime {
        store: &fixture.store,
        active_generation: fixture.generation.generation_id.clone(),
        state_dir: fixture.state_dir.clone(),
    };
    let context = McpServeContext {
        repository_root: fixture.repository_root.clone(),
        state_dir_override: Some(fixture.state_dir.clone()),
    };

    eprintln!(
        "{{\"benchmark\":\"read_path_fixture\",\"code_units\":{},\"families\":{},\"evidence\":{},\"semantic_facts\":{}}}",
        READ_PATH_BENCH_CODE_UNITS,
        READ_PATH_BENCH_FAMILIES,
        READ_PATH_BENCH_EVIDENCE,
        READ_PATH_BENCH_SEMANTIC_FACTS
    );

    let (stats, duration) = measure_read_path("stats_json", || {
        repo_shape_diagnostics(&fixture.store, &fixture.store)
    });
    let stats = stats.expect("stats diagnostics");
    assert_eq!(stats.eligible_code_units, READ_PATH_BENCH_CODE_UNITS);
    assert_eq!(stats.family_count, READ_PATH_BENCH_FAMILIES);
    print_read_path_measurement("stats_json", duration);

    let (unknowns, duration) =
        measure_read_path("stats_unknowns_json", || unknown_inventory(&fixture.store));
    unknowns.expect("unknown inventory");
    print_read_path_measurement("stats_unknowns_json", duration);

    let (families, duration) = measure_read_path("families_json", || list_families(&fixture.store));
    let families = families.expect("family summaries");
    assert_eq!(families.families.len(), READ_PATH_BENCH_FAMILIES);
    print_read_path_measurement("families_json", duration);

    let (lookup, duration) = measure_read_path("lookup_exact_family", || {
        lookup_family(
            &fixture.store,
            Some(&fixture.exact_family_id),
            FamilyLookupMode::ExactFamilyId,
        )
    });
    assert_found_family(lookup.expect("exact family lookup"));
    print_read_path_measurement("lookup_exact_family", duration);

    let (lookup, duration) = measure_read_path("lookup_exact_member", || {
        lookup_family(
            &fixture.store,
            Some(&fixture.exact_member_id),
            FamilyLookupMode::ExactMemberId,
        )
    });
    assert_found_family(lookup.expect("exact member lookup"));
    print_read_path_measurement("lookup_exact_member", duration);

    let (lookup, duration) = measure_read_path("lookup_fuzzy_path", || {
        lookup_family(
            &fixture.store,
            Some(&fixture.fuzzy_path),
            FamilyLookupMode::FuzzyQuery,
        )
    });
    assert_found_family(lookup.expect("fuzzy path lookup"));
    print_read_path_measurement("lookup_fuzzy_path", duration);

    let (response, duration) = measure_read_path("mcp_show_family_exact", || {
        mcp_context_call(&runtime, &context, "show_family", &fixture.exact_family_id)
    });
    assert_mcp_ok(response.expect("mcp exact family"));
    print_read_path_measurement("mcp_show_family_exact", duration);

    let (response, duration) = measure_read_path("mcp_exact_member", || {
        mcp_context_call(
            &runtime,
            &context,
            "find_analogues",
            &fixture.exact_member_id,
        )
    });
    assert_mcp_ok(response.expect("mcp exact member"));
    print_read_path_measurement("mcp_exact_member", duration);

    let (response, duration) = measure_read_path("mcp_fuzzy_path", || {
        mcp_context_call(&runtime, &context, "find_analogues", &fixture.fuzzy_path)
    });
    assert_mcp_ok(response.expect("mcp fuzzy path"));
    print_read_path_measurement("mcp_fuzzy_path", duration);
}

struct ReadPathBenchmarkFixture {
    _workspace: TempWorkspace,
    store: SqliteIndexStore,
    generation: GenerationHandle,
    repository_root: String,
    state_dir: String,
    exact_family_id: String,
    exact_member_id: String,
    fuzzy_path: String,
}

fn build_read_path_benchmark_fixture() -> ReadPathBenchmarkFixture {
    let workspace = TempWorkspace::new("read-path-benchmark");
    let state_dir = workspace.path().join(".repogrammar");
    let state_dir_string = state_dir.display().to_string();
    let repository_root = workspace.path().display().to_string();
    let store = SqliteIndexStore::new(&state_dir);
    let generation = store
        .prepare_next_generation()
        .expect("prepare benchmark generation");
    let mut units = Vec::with_capacity(READ_PATH_BENCH_CODE_UNITS);

    for index in 0..READ_PATH_BENCH_CODE_UNITS {
        let file = benchmark_file(index);
        let unit = benchmark_unit(&file, index);
        store
            .record_indexed_file(&generation, &file)
            .expect("record benchmark file");
        store
            .record_code_unit(&generation, &unit)
            .expect("record benchmark code unit");
        units.push(unit);
    }

    for index in 0..READ_PATH_BENCH_SEMANTIC_FACTS {
        let unit = &units[index % units.len()];
        store
            .record_semantic_fact(&generation, &benchmark_semantic_fact(unit, index))
            .expect("record benchmark semantic fact");
    }

    for (index, unit) in units.iter().enumerate().take(READ_PATH_BENCH_FAMILIES) {
        let family = benchmark_family(index);
        store
            .record_family(&generation, &family)
            .expect("record benchmark family");
        store
            .record_family_member(&generation, &benchmark_family_member(&family, unit))
            .expect("record benchmark family member");
    }

    for index in 0..READ_PATH_BENCH_EVIDENCE {
        let family = benchmark_family(index % READ_PATH_BENCH_FAMILIES);
        let unit = &units[index % units.len()];
        store
            .record_family_evidence(
                &generation,
                &benchmark_family_evidence(&family, unit, index),
            )
            .expect("record benchmark family evidence");
    }

    store
        .activate_generation(&generation)
        .expect("activate benchmark generation");

    let exact_family = benchmark_family(0);
    let exact_member_id = units[0].id.clone();
    let fuzzy_path = units[0].path.clone();
    ReadPathBenchmarkFixture {
        _workspace: workspace,
        store,
        generation,
        repository_root,
        state_dir: state_dir_string,
        exact_family_id: exact_family.family_id,
        exact_member_id,
        fuzzy_path,
    }
}

fn benchmark_file(index: usize) -> IndexedFileRecord {
    IndexedFileRecord {
        path: format!("src/bench/unit_{index:05}.py"),
        content_hash: benchmark_hash(index),
        size_bytes: 128,
        language: "python".to_string(),
    }
}

fn benchmark_unit(file: &IndexedFileRecord, index: usize) -> IndexedCodeUnitRecord {
    IndexedCodeUnitRecord {
        id: format!("unit:{}#fastapi_route:0-64", file.path),
        path: file.path.clone(),
        language: file.language.clone(),
        kind: "fastapi_route".to_string(),
        start_byte: 0,
        end_byte: 64,
        content_hash: benchmark_hash(index),
    }
}

fn benchmark_semantic_fact(
    unit: &IndexedCodeUnitRecord,
    index: usize,
) -> IndexedSemanticFactRecord {
    IndexedSemanticFactRecord {
        fact_id: format!("semantic-fact:{index:05}"),
        kind: "RESOLVED_CALL".to_string(),
        subject: format!("{}#call:{index}", unit.path),
        target: Some(format!("{}#target:{index}", unit.path)),
        certainty: "SEMANTIC".to_string(),
        origin_engine: "read_path_benchmark".to_string(),
        origin_engine_version: "1".to_string(),
        origin_method: "fixture".to_string(),
        assumptions: Vec::new(),
        evidence_id: format!("semantic-evidence:{index:05}"),
        code_unit_id: unit.id.clone(),
        path: unit.path.clone(),
        content_hash: unit.content_hash.clone(),
        start_byte: 0,
        end_byte: 16,
        note: "benchmark semantic fact".to_string(),
    }
}

fn benchmark_family(index: usize) -> IndexedFamilyRecord {
    IndexedFamilyRecord {
        family_id: format!("family:python:read_path_benchmark:family_{index:03}"),
        classification: "DOMINANT_PATTERN".to_string(),
        prevalence: crate::test_support::sample_family_prevalence(),
    }
}

fn benchmark_family_member(
    family: &IndexedFamilyRecord,
    unit: &IndexedCodeUnitRecord,
) -> IndexedFamilyMemberRecord {
    IndexedFamilyMemberRecord {
        family_id: family.family_id.clone(),
        code_unit_id: unit.id.clone(),
        role: "framework:benchmark.route_handler".to_string(),
    }
}

fn benchmark_family_evidence(
    family: &IndexedFamilyRecord,
    unit: &IndexedCodeUnitRecord,
    index: usize,
) -> IndexedFamilyEvidenceRecord {
    IndexedFamilyEvidenceRecord {
        evidence_id: format!("family-evidence:{index:05}"),
        family_id: family.family_id.clone(),
        code_unit_id: unit.id.clone(),
        covered_claims: vec!["support".to_string()],
        path: unit.path.clone(),
        content_hash: unit.content_hash.clone(),
        start_byte: 0,
        end_byte: 16,
        note: "benchmark family evidence".to_string(),
    }
}

fn benchmark_hash(index: usize) -> ContentHash {
    ContentHash::new(format!("sha256:{index:064x}")).expect("valid benchmark hash")
}

fn measure_read_path<T>(name: &str, query: impl FnOnce() -> T) -> (T, Duration) {
    let started = Instant::now();
    let result = query();
    let duration = started.elapsed();
    eprintln!(
        "{{\"benchmark\":\"read_path_measurement\",\"name\":\"{name}\",\"elapsed_ms\":{:.3}}}",
        duration.as_secs_f64() * 1000.0
    );
    (result, duration)
}

fn print_read_path_measurement(name: &str, duration: Duration) {
    eprintln!(
        "{{\"benchmark\":\"read_path_result\",\"name\":\"{name}\",\"elapsed_ms\":{:.3}}}",
        duration.as_secs_f64() * 1000.0
    );
}

fn assert_found_family(report: FamilyLookupReport) {
    let FamilyLookupReport::Found(_) = report else {
        panic!("benchmark lookup should resolve to one family");
    };
}

fn mcp_context_call(
    runtime: &impl McpReadOnlyRuntime,
    context: &McpServeContext,
    operation: &str,
    target: &str,
) -> Result<Value, String> {
    handle_context_call(
        runtime,
        context,
        &json!({
            "operation": operation,
            "target": target,
            "mode": "compact",
        }),
    )
    .map_err(|error| format!("MCP protocol error {}: {}", error.code(), error.message()))
}

fn assert_mcp_ok(response: Value) {
    assert_eq!(response["status"], "ok", "{response}");
}

struct BenchmarkMcpRuntime<'a> {
    store: &'a SqliteIndexStore,
    active_generation: String,
    state_dir: String,
}

impl McpReadOnlyRuntime for BenchmarkMcpRuntime<'_> {
    fn repository_status(
        &self,
        _request: RepositoryStatusRequest,
    ) -> Result<RepositoryStatusReport, crate::error::RepoGrammarError> {
        Ok(RepositoryStatusReport {
            state_dir: self.state_dir.clone(),
            status: RepositoryStatus::Initialized {
                active_generation: self.active_generation.clone(),
            },
            manifest: RepositoryManifestStatus::Valid,
            manifest_schema_version: Some(1),
            missing_subdirs: Vec::new(),
            storage: RepositoryImplementationStatus::Available,
            indexing: RepositoryImplementationStatus::SyntaxOnlyCodeUnits,
            storage_inspection: None,
            storage_error: None,
            readiness: RepositoryReadiness::default(),
        })
    }

    fn family_lookup(
        &self,
        _request: RepositoryStatusRequest,
        target: Option<&str>,
        mode: FamilyLookupMode,
    ) -> Result<FamilyLookupReport, crate::error::RepoGrammarError> {
        lookup_family(self.store, target, mode)
    }
}

#[test]
fn constraint_profile_round_trips_through_storage_use_cases() {
    let workspace = TempWorkspace::new("integration-constraint-profile");
    let state_dir = workspace.path().join(".repogrammar");
    let store = SqliteIndexStore::new(&state_dir);
    let generation = store.prepare_next_generation().expect("prepare generation");

    let file = benchmark_file(0);
    let unit = benchmark_unit(&file, 0);
    store
        .record_indexed_file(&generation, &file)
        .expect("record file");
    store
        .record_code_unit(&generation, &unit)
        .expect("record code unit");
    let family = benchmark_family(0);
    store
        .record_family(&generation, &family)
        .expect("record family");
    store
        .record_family_member(&generation, &benchmark_family_member(&family, &unit))
        .expect("record family member");
    store
        .record_family_evidence(&generation, &benchmark_family_evidence(&family, &unit, 0))
        .expect("record family evidence");

    let record = crate::ports::family_store::IndexedFamilyConstraintProfileRecord {
        family_id: family.family_id.clone(),
        profile: crate::test_support::sample_family_constraint_profile(),
    };
    // The write path runs through the storage use-case (validation) into the
    // real SQLite adapter, proving the full stack round-trips a profile.
    crate::application::storage::record_family_constraint_profile(&store, &generation, &record)
        .expect("record constraint profile through the storage use case");
    store
        .activate_generation(&generation)
        .expect("activate generation");

    let hydrated =
        crate::application::storage::show_family_constraint_profile(&store, &family.family_id)
            .expect("show constraint profile through the storage use case")
            .expect("profile exists after activation");
    assert_eq!(
        hydrated,
        crate::test_support::sample_family_constraint_profile()
    );
}
