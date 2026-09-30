"""Offline development-task freeze and static reference/mutant qualification.

Never executes fixture code, framework runtimes, agents, or external commands.
Exact task/seed/answer/rubric bundles belong outside every agent worktree. These
newly authored and tested tasks are burned development fixtures, not held-out
adoption-effect evidence. AST checks do not prove runtime authentication.
"""

from __future__ import annotations

import argparse
import ast
import copy
import hashlib
import json
from pathlib import Path
import re

from treehash import tree_sha256


ROOT = Path(__file__).resolve().parents[3]
KINDS = ("FASTAPI", "PYTEST", "PYDANTIC", "SQLALCHEMY", "DOCUMENTATION",
         "EXACT_LOOKUP", "UNKNOWN", "STALE")

ROUTES = '''from fastapi import APIRouter
from .access import CatalogAccess
from .services import labels_for, weight_for

router = APIRouter(prefix="/packages")

@router.get("/{package_code}/weight", response_model=int, tags=["catalog"])
def package_weight(package_code: str, access: CatalogAccess):
    return weight_for(package_code)
'''
NEW_ROUTE = '''
@router.get("/{package_code}/labels", response_model=list[str], tags=["catalog"])
def package_labels(package_code: str, access: CatalogAccess):
    return labels_for(package_code)
'''
CONFTEST = '''import pytest
from .bundles import create_bundle

@pytest.fixture(name="sample_bundle")
def _sample_bundle(tmp_path):
    bundle = create_bundle(tmp_path, "sample")
    try:
        yield bundle
    finally:
        bundle.close()
'''
NEW_FIXTURE = '''
@pytest.fixture(name="catalog_bundle")
def _catalog_bundle(tmp_path):
    bundle = create_bundle(tmp_path, "catalog")
    try:
        yield bundle
    finally:
        bundle.close()
'''
MODELS = '''from pydantic import BaseModel, ConfigDict, Field, field_validator

class StrictPayload(BaseModel):
    model_config = ConfigDict(extra="forbid", frozen=True)

class ParcelDraft(StrictPayload):
    parcel_count: int = Field(ge=1, le=12)
    warehouse_code: str

    @field_validator("warehouse_code", mode="before")
    @classmethod
    def normalize_warehouse(cls, value):
        return value.strip().upper()
'''
NEW_MODEL = '''
class LabelBatch(StrictPayload):
    label_count: int = Field(ge=1, le=40)
    warehouse_code: str

    @field_validator("warehouse_code", mode="before")
    @classmethod
    def normalize_warehouse(cls, value):
        return value.strip().upper()
'''
STORE = '''from sqlalchemy import select
from .entities import Shipment

class ShipmentStore:
    def __init__(self, session):
        self.session = session

    def by_status(self, status: str) -> list[Shipment]:
        stmt = select(Shipment).where(Shipment.status == status).order_by(Shipment.id)
        return list(self.session.scalars(stmt))
'''
NEW_METHOD = '''
    def by_destination(self, destination_code: str) -> list[Shipment]:
        stmt = select(Shipment).where(Shipment.destination_code == destination_code).order_by(Shipment.id)
        return list(self.session.scalars(stmt))
'''


def frozen_tasks() -> tuple[dict, ...]:
    """Fresh private copies; never deliver reference/mutants/rubrics to agents."""
    tasks = [
        {"kind": "FASTAPI", "eligible": True,
         "prompt": "Extend the package catalog with a labels operation. Locate its existing weight operation and preserve its registration, access parameter and tagging conventions. Add package_labels with the same package_code parameter, a list[str] response model and a /{package_code}/labels suffix, delegating directly to labels_for(package_code). Leave existing definitions intact; edit only app/catalog.py.",
         "seed": {"app/catalog.py": ROUTES,
                  "app/hooks.py": 'from fastapi import FastAPI\napp = FastAPI()\n@app.post("/incoming")\ndef accept_hook(payload: dict):\n    return payload\n'},
         "reference": {"files": {"app/catalog.py": ROUTES + NEW_ROUTE}, "answer": {}},
         "mutants": [
             {"kind": "MISSED_WORK", "files": {}, "answer": {}},
             {"kind": "WRONG_ANALOGUE", "files": {"app/catalog.py": ROUTES + NEW_ROUTE.replace("router.get", "router.post")}, "answer": {}},
             {"kind": "CONVENTION", "files": {"app/catalog.py": ROUTES + NEW_ROUTE.replace(", access: CatalogAccess", "")}, "answer": {}},
         ]},
        {"kind": "PYTEST", "eligible": True,
         "prompt": "Provide a pytest fixture registered as catalog_bundle for package tests. Follow the lifecycle of the sample bundle fixture, including teardown if a consuming test fails. Add _catalog_bundle using tmp_path and create_bundle(tmp_path, 'catalog'); keep default function scope. Change only tests/conftest.py and preserve existing definitions.",
         "seed": {"tests/conftest.py": CONFTEST,
                  "tests/shared.py": 'import pytest\n@pytest.fixture(scope="session")\ndef static_bundle():\n    return {"shared": True}\n'},
         "reference": {"files": {"tests/conftest.py": CONFTEST + NEW_FIXTURE}, "answer": {}},
         "mutants": [
             {"kind": "MISSED_WORK", "files": {}, "answer": {}},
             {"kind": "WRONG_ANALOGUE", "files": {"tests/conftest.py": CONFTEST + '\n@pytest.fixture(name="catalog_bundle")\ndef _catalog_bundle(tmp_path):\n    return create_bundle(tmp_path, "catalog")\n'}, "answer": {}},
             {"kind": "CONVENTION", "files": {"tests/conftest.py": CONFTEST + NEW_FIXTURE.replace('name="catalog_bundle"', 'name="catalog_bundle", scope="session"')}, "answer": {}},
         ]},
        {"kind": "PYDANTIC", "eligible": True,
         "prompt": "Introduce LabelBatch alongside the parcel payload in app/payloads.py. Use this package's strict immutable payload base and warehouse normalization convention. Require label_count as an int bounded inclusively from 1 to 40, and warehouse_code as str. Retain all existing definitions and imports; add only this class.",
         "seed": {"app/payloads.py": MODELS,
                  "app/legacy.py": 'from dataclasses import dataclass\n@dataclass\nclass LooseLabelBatch:\n    label_count: int\n    warehouse_code: str\n'},
         "reference": {"files": {"app/payloads.py": MODELS + NEW_MODEL}, "answer": {}},
         "mutants": [
             {"kind": "MISSED_WORK", "files": {}, "answer": {}},
             {"kind": "WRONG_ANALOGUE", "files": {"app/payloads.py": MODELS + NEW_MODEL.replace("StrictPayload):", "BaseModel):")}, "answer": {}},
             {"kind": "CONVENTION", "files": {"app/payloads.py": MODELS + NEW_MODEL.replace("le=40", "le=400")}, "answer": {}},
         ]},
        {"kind": "SQLALCHEMY", "eligible": True,
         "prompt": "Extend ShipmentStore in app/shipment_store.py with by_destination(self, destination_code: str) -> list[Shipment]. Follow its by_status query convention, filtering Shipment.destination_code by the supplied argument and preserving ascending Shipment.id order. Reuse the injected session and materialize scalar results. Keep the existing store unchanged apart from the added method.",
         "seed": {"app/shipment_store.py": STORE,
                  "app/legacy_store.py": 'class LegacyStore:\n    def by_destination(self, destination_code):\n        return self.session.get(destination_code)\n'},
         "reference": {"files": {"app/shipment_store.py": STORE + NEW_METHOD}, "answer": {}},
         "mutants": [
             {"kind": "MISSED_WORK", "files": {}, "answer": {}},
             {"kind": "WRONG_ANALOGUE", "files": {"app/shipment_store.py": STORE + NEW_METHOD.replace("Shipment.destination_code", "Shipment.status")}, "answer": {}},
             {"kind": "CONVENTION", "files": {"app/shipment_store.py": STORE + NEW_METHOD.replace(".order_by(Shipment.id)", "")}, "answer": {}},
         ]},
        {"kind": "DOCUMENTATION", "eligible": False,
         "prompt": "Correct the displayed archive size in README.md from 18 KiB to 24 KiB. Preserve every other byte, including the final newline. Make no code changes.",
         "seed": {"README.md": "Package archive\n\nArchive size: 18 KiB\n",
                  "app/size.py": "ARCHIVE_SIZE = 18\n"},
         "reference": {"files": {"README.md": "Package archive\n\nArchive size: 24 KiB\n"}, "answer": {}},
         "mutants": [
             {"kind": "MISSED_WORK", "files": {}, "answer": {}},
             {"kind": "WRONG_TARGET", "files": {"app/size.py": "ARCHIVE_SIZE = 24\n"}, "answer": {}},
             {"kind": "SCOPE", "files": {"README.md": "Package archive\n\nArchive size: 24 KiB\n", "app/size.py": "ARCHIVE_SIZE = 24\n"}, "answer": {}},
         ]},
        {"kind": "EXACT_LOOKUP", "eligible": False,
         "prompt": "In app/transfer.py, identify the function whose first parameter is archive_token. Return only JSON with a function key. Leave all files unchanged.",
         "seed": {"app/transfer.py": 'def receive_archive(archive_token, destination):\n    return destination\n\ndef cancel_transfer(transfer_token):\n    return False\n'},
         "reference": {"files": {}, "answer": {"function": "receive_archive"}},
         "mutants": [
             {"kind": "MISSED_WORK", "files": {}, "answer": {}},
             {"kind": "WRONG_TARGET", "files": {}, "answer": {"function": "cancel_transfer"}},
             {"kind": "SCOPE", "files": {"app/transfer.py": ""}, "answer": {"function": "receive_archive"}},
         ]},
        {"kind": "UNKNOWN", "eligible": True,
         "prompt": "Determine whether app/runtime_bindings.py statically proves a FastAPI route registration convention. Return only JSON with decision and reason keys. Use UNKNOWN with dynamic_registration when runtime-selected registration prevents a source-grounded framework conclusion. Do not change files or claim an inspected candidate proves a family.",
         "seed": {"app/runtime_bindings.py": 'from .runtime_registry import choose_registrar\nregister = choose_registrar()\n@register("/dispatch")\ndef dispatch_parcel(payload):\n    return payload\n'},
         "reference": {"files": {}, "answer": {"decision": "UNKNOWN", "reason": "dynamic_registration"}},
         "mutants": [
             {"kind": "MISSED_WORK", "files": {}, "answer": {}},
             {"kind": "CANDIDATE_AS_PROOF", "files": {}, "answer": {"decision": "KNOWN", "reason": "candidate_fastapi"}},
             {"kind": "SCOPE", "files": {"app/runtime_bindings.py": ""}, "answer": {"decision": "UNKNOWN", "reason": "dynamic_registration"}},
         ]},
        {"kind": "STALE", "eligible": True,
         "prompt": "Identify the current registration of publish_manifest in app/manifest.py. The prepared index predates a source change; any historical registration must be checked against current source. Return only JSON with method and path keys from the current decorator; do not change files.",
         "seed": {"app/manifest.py": 'from fastapi import APIRouter\nrouter = APIRouter()\n@router.post("/manifest")\ndef publish_manifest(payload: dict):\n    return payload\n'},
         "index_seed": {"app/manifest.py": 'from fastapi import APIRouter\nrouter = APIRouter()\n@router.get("/manifest-old")\ndef publish_manifest():\n    return {}\n'},
         "reference": {"files": {}, "answer": {"method": "post", "path": "/manifest"}},
         "mutants": [
             {"kind": "MISSED_WORK", "files": {}, "answer": {}},
             {"kind": "STALE_ANSWER", "files": {}, "answer": {"method": "get", "path": "/manifest-old"}},
             {"kind": "SCOPE", "files": {"app/manifest.py": ""}, "answer": {"method": "post", "path": "/manifest"}},
         ]},
    ]
    for ordinal, task in enumerate(tasks):
        task["task_id"] = f"D{ordinal + 1:02d}"
        task["split"] = "DEVELOPMENT_BURNED"
    return tuple(copy.deepcopy(tasks))


def _ast(source: str) -> str:
    return ast.dump(ast.parse(source), include_attributes=False)


def _appended_node(seed: str, actual: str, node_type: type, name: str) -> ast.AST | None:
    old, new = ast.parse(seed), ast.parse(actual)
    if len(new.body) != len(old.body) + 1:
        return None
    added = new.body.pop()
    if ast.dump(old) != ast.dump(new) or not isinstance(added, node_type) or added.name != name:
        return None
    return added


def _function(node: ast.AST, parameters: tuple[str, ...], body: tuple[str, ...]) -> bool:
    """Bounded static recipe: argument shape and statements, ignoring formatting."""
    if not isinstance(node, ast.FunctionDef):
        return False
    args = node.args
    return (tuple(ast.unparse(arg) for arg in args.args) == parameters
            and not (args.defaults or args.posonlyargs or args.kwonlyargs or args.vararg or args.kwarg)
            and tuple(ast.unparse(statement) for statement in node.body) == body)


def _route_contract(node: ast.AST | None) -> bool:
    if not _function(node, ("package_code: str", "access: CatalogAccess"), ("return labels_for(package_code)",)):
        return False
    decorators = node.decorator_list
    if len(decorators) != 1 or not isinstance(decorators[0], ast.Call):
        return False
    route = decorators[0]
    return (ast.unparse(route.func) == "router.get" and len(route.args) == 1
            and isinstance(route.args[0], ast.Constant) and route.args[0].value == "/{package_code}/labels"
            and len(route.keywords) == 2
            and {k.arg: ast.unparse(k.value) for k in route.keywords}
            == {"response_model": "list[str]", "tags": "['catalog']"})


def _fixture_contract(node: ast.AST | None) -> bool:
    if not isinstance(node, ast.FunctionDef) or len(node.body) != 2:
        return False
    lifecycle = node.body[1]
    return (tuple(ast.unparse(arg) for arg in node.args.args) == ("tmp_path",)
            and not (node.args.defaults or node.args.posonlyargs or node.args.kwonlyargs or node.args.vararg or node.args.kwarg)
            and tuple(ast.unparse(d) for d in node.decorator_list) == ("pytest.fixture(name='catalog_bundle')",)
            and ast.unparse(node.body[0]) == "bundle = create_bundle(tmp_path, 'catalog')"
            and isinstance(lifecycle, ast.Try) and not (lifecycle.handlers or lifecycle.orelse)
            and tuple(ast.unparse(n) for n in lifecycle.body) == ("yield bundle",)
            and tuple(ast.unparse(n) for n in lifecycle.finalbody) == ("bundle.close()",))


def _model_contract(node: ast.AST | None) -> bool:
    if not isinstance(node, ast.ClassDef) or len(node.body) != 3:
        return False
    validator = node.body[-1]
    return (tuple(ast.unparse(b) for b in node.bases) == ("StrictPayload",)
            and not (node.keywords or node.decorator_list)
            and ast.unparse(node.body[0]) == "label_count: int = Field(ge=1, le=40)"
            and ast.unparse(node.body[1]) == "warehouse_code: str"
            and _function(validator, ("cls", "value"), ("return value.strip().upper()",))
            and validator.name == "normalize_warehouse"
            and tuple(ast.unparse(d) for d in validator.decorator_list)
            == ("field_validator('warehouse_code', mode='before')", "classmethod"))


def _store_contract(seed: str, source: str) -> bool:
    old, actual = ast.parse(seed), ast.parse(source)
    store = actual.body[-1]
    if not isinstance(store, ast.ClassDef) or store.name != "ShipmentStore":
        return False
    method = store.body.pop()
    return (ast.dump(actual) == ast.dump(old) and method.name == "by_destination"
            and _function(method, ("self", "destination_code: str"),
                          ("stmt = select(Shipment).where(Shipment.destination_code == destination_code).order_by(Shipment.id)",
                           "return list(self.session.scalars(stmt))"))
            and not method.decorator_list and ast.unparse(method.returns) == "list[Shipment]")


def assess(task: dict, files: dict[str, str], answer: dict) -> dict:
    """Grade a complete in-memory worktree, never execute its untrusted code.

    Static exact AST contracts admit formatting changes, not arbitrary equivalent
    rewrites. That narrow development ceiling is deliberate; blind review and a
    real held-out freeze remain separate future gates.
    """
    kind, seed = task["kind"], task["seed"]
    if kind not in KINDS:
        raise ValueError("unsupported task kind")
    if (not isinstance(files, dict) or not isinstance(answer, dict)
            or any(not isinstance(k, str) or not isinstance(v, str) for k, v in files.items())):
        raise ValueError("files and answer must have the frozen input shapes")
    changed = {p for p in set(seed) | set(files) if files.get(p) != seed.get(p)}
    target = next(iter(task["reference"]["files"]), None)
    scope_ok = changed == ({target} if target else set())
    accepted = False
    try:
        if kind == "FASTAPI":
            node = _appended_node(seed[target], files.get(target, ""), ast.FunctionDef, "package_labels")
            accepted = _route_contract(node)
        elif kind == "PYTEST":
            node = _appended_node(seed[target], files.get(target, ""), ast.FunctionDef, "_catalog_bundle")
            accepted = _fixture_contract(node)
        elif kind == "PYDANTIC":
            node = _appended_node(seed[target], files.get(target, ""), ast.ClassDef, "LabelBatch")
            accepted = _model_contract(node)
        elif kind == "SQLALCHEMY":
            # The addition belongs inside the existing class, not at module scope.
            accepted = _store_contract(seed[target], files.get(target, ""))
        elif kind == "DOCUMENTATION":
            accepted = files.get(target) == "Package archive\n\nArchive size: 24 KiB\n" and answer == {}
        elif kind == "EXACT_LOOKUP":
            functions = [n for n in ast.parse(seed["app/transfer.py"]).body
                         if isinstance(n, ast.FunctionDef) and n.args.args[0].arg == "archive_token"]
            accepted = len(functions) == 1 and answer == {"function": functions[0].name}
        elif kind == "UNKNOWN":
            accepted = answer == {"decision": "UNKNOWN", "reason": "dynamic_registration"}
        elif kind == "STALE":
            function = next(n for n in ast.parse(seed["app/manifest.py"]).body
                            if isinstance(n, ast.FunctionDef) and n.name == "publish_manifest")
            decorator = function.decorator_list[0]
            accepted = answer == {"method": decorator.func.attr, "path": decorator.args[0].value}
    except (SyntaxError, ValueError, IndexError, AttributeError, StopIteration):
        accepted = False
    if kind in KINDS[:4]:
        accepted = accepted and answer == {}
    return {"status": "PASS" if accepted and scope_ok else "FAIL",
            "scope": "PASS" if scope_ok else "FAIL",
            "static_contract": "PASS" if accepted else "FAIL"}


def _sha(value: object) -> str:
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(",", ":")).encode()).hexdigest()


def _phrases(text: str) -> set[tuple[str, ...]]:
    words = re.findall(r"[a-z0-9_]+", text.lower())
    return {tuple(words[i:i + 8]) for i in range(len(words) - 7)}


def screen(tasks: tuple[dict, ...]) -> dict:
    """Reproducible lexical/exact-AST screen; not proof of semantic independence."""
    paths = [ROOT / "src/fixtures/evaluation/query-corpus-v1.json",
             ROOT / "src/experiments/agent_study/tasks/t1_item_summary.json",
             ROOT / "src/experiments/agent_study/tasks/t6_db_session.json",
             ROOT / "docs/experiments/data/agent-adoption-plan.v1.json"]
    corpus = json.loads(paths[0].read_text(encoding="utf-8"))
    prompts = [q["target"] for q in corpus["queries"]]
    prompts += [json.loads(p.read_text(encoding="utf-8"))["prompt"] for p in paths[1:3]]
    prompts += [t["prompt"] for t in json.loads(paths[3].read_text(encoding="utf-8"))["tasks"]]
    source_roots = [ROOT / "src/experiments/agent_study/fixtures/mini_repo"]
    source_roots += [ROOT / f["root"] for f in corpus["fixtures"]]
    source_asts = set()
    source_count = 0
    for root in source_roots:
        for path in sorted(root.rglob("*.py")):
            if path.is_symlink():
                raise ValueError("screen source cannot be symlinked")
            source_count += 1
            try:
                source_asts.add(_ast(path.read_text(encoding="utf-8")))
            except SyntaxError:
                # Malformed negative fixtures are still frozen by their tree hash.
                pass
    historical_phrases = set().union(*(_phrases(p) for p in prompts))
    phrase_matches = sum(bool(_phrases(t["prompt"]) & historical_phrases) for t in tasks)
    source_matches = sum(_ast(s) in source_asts for t in tasks for s in t["seed"].values()
                         if s and s != t["seed"].get("README.md"))
    identities = [hashlib.sha256(p.read_bytes()).hexdigest() for p in paths]
    identities += [tree_sha256(str(root)) for root in source_roots]
    return {"status": "PASS" if phrase_matches == source_matches == 0 else "FAIL",
            "mode": "LEXICAL_8_TOKEN_AND_EXACT_AST", "input_sha256": _sha(identities),
            "historical_statement_count": len(prompts), "historical_source_count": source_count,
            "phrase_overlap_task_count": phrase_matches, "exact_ast_overlap_file_count": source_matches,
            "semantic_independence": "NOT_PROVEN"}


def qualify(output_dir: Path) -> dict:
    """Freeze private bundles and emit only a closed source-free summary."""
    raw = Path(output_dir).absolute()
    if raw.is_symlink():
        raise ValueError("output directory cannot be symlinked")
    out = raw.resolve()
    if out == ROOT or ROOT in out.parents:
        raise ValueError("private bundles must be outside the repository")
    if out.exists():
        raise ValueError("output directory must be new")
    tasks = frozen_tasks()
    screening = screen(tasks)
    if screening["status"] != "PASS":
        raise ValueError("development suite overlaps burned screening inputs")
    rows = []
    for task in tasks:
        reference = task["reference"]
        reference_status = assess(task, task["seed"] | reference["files"], reference["answer"])["status"]
        mutants = [{"kind": m["kind"], "bundle_sha256": _sha(m),
                    "status": assess(task, task["seed"] | m["files"], m["answer"])["status"]}
                   for m in task["mutants"]]
        rows.append({"task_id": task["task_id"], "kind": task["kind"], "eligible": task["eligible"],
                     "split": "DEVELOPMENT_BURNED", "task_sha256": _sha(task),
                     "statement_sha256": hashlib.sha256(task["prompt"].encode()).hexdigest(),
                     "seed_sha256": _sha(task["seed"]),
                     "index_seed_sha256": _sha(task.get("index_seed", task["seed"])),
                     "reference_sha256": _sha(reference), "reference_status": reference_status,
                     "mutants": mutants})
    status = "PASS" if all(r["reference_status"] == "PASS" and all(
        m["status"] == "FAIL" for m in r["mutants"]) for r in rows) else "FAIL"
    summary = {"schema_version": "agent-adoption-oracle-qualification.v2", "status": status,
               "split": "DEVELOPMENT_BURNED", "screen": screening, "tasks": rows,
               "task_count": len(rows), "reference_pass_count": sum(r["reference_status"] == "PASS" for r in rows),
               "mutant_reject_count": sum(m["status"] == "FAIL" for r in rows for m in r["mutants"]),
               "oracle_source_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
               "treehash_source_sha256": hashlib.sha256(Path(__file__).with_name("treehash.py").read_bytes()).hexdigest(),
               "bundle_sha256": _sha(tasks), "runtime_auth": "NOT_MEASURED",
               "live_unknown_stale_handling": "NOT_MEASURED", "held_out_effectiveness": "NOT_MEASURED"}
    out.mkdir(parents=True, mode=0o700)
    private = out / "private"
    private.mkdir(mode=0o700)
    (private / "tasks.json").write_text(json.dumps(tasks, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    (private / "tasks.json").chmod(0o600)
    (private / "oracle.py").write_bytes(Path(__file__).read_bytes())
    (private / "oracle.py").chmod(0o600)
    (private / "treehash.py").write_bytes(Path(__file__).with_name("treehash.py").read_bytes())
    (private / "treehash.py").chmod(0o600)
    (out / "qualification.summary.json").write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    if status != "PASS":
        raise ValueError("reference/mutant qualification failed; retained failure summary")
    return summary


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", required=True, type=Path)
    args = parser.parse_args()
    result = qualify(args.out)
    print(json.dumps(result, sort_keys=True))
    raise SystemExit(0 if result["status"] == "PASS" else 1)
