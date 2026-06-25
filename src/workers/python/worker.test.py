#!/usr/bin/env python3
"""Smoke tests for the dependency-free Python worker."""

from __future__ import annotations

import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

WORKER = Path(__file__).with_name("worker.py")


def run_worker(payload):
    data = payload if isinstance(payload, str) else json.dumps(payload) + "\n"
    result = subprocess.run(
        [sys.executable, str(WORKER)],
        input=data,
        text=True,
        capture_output=True,
        check=False,
    )
    assert result.returncode == 0, result.stderr
    assert result.stderr == ""
    return [json.loads(line) for line in result.stdout.splitlines() if line.strip()]


def valid_request(root: str):
    return {
        "protocol_version": 1,
        "request_id": "repogrammar-python-semantic-worker",
        "project_root": root,
        "changed_files": ["app.py"],
    }


def assert_end_of_stream(messages):
    assert messages[-1] == {
        "protocol_version": 1,
        "message_type": "end_of_stream",
        "request_id": "repogrammar-python-semantic-worker",
    }


parse_messages = run_worker(
    {
        "protocol_version": 1,
        "mode": "parse_document",
        "path": "app.py",
        "content_hash": "sha256:" + "0" * 64,
        "repository_revision": "UNKNOWN",
        "text": """
from fastapi import APIRouter
from pydantic import BaseModel
from sqlalchemy.orm import DeclarativeBase, Mapped, mapped_column

router = APIRouter()

class UserOut(BaseModel):
    id: int

class Base(DeclarativeBase):
    pass

class User(Base):
    id: Mapped[int] = mapped_column(primary_key=True)

@router.get("/users")
async def list_users():
    return []

def test_users(client):
    assert client.get("/users").status_code == 200
""",
    }
)
assert len(parse_messages) == 1
unit_kinds = [unit["kind"] for unit in parse_messages[0]["units"]]
assert "module" in unit_kinds
assert "fastapi_route" in unit_kinds
assert "pytest_test" in unit_kinds
assert "pydantic_model" in unit_kinds
assert "sqlalchemy_model" in unit_kinds
assert "async_function" not in unit_kinds
parse_facts = parse_messages[0]["facts"]
assert any(fact["fact_kind"] == "RESOLVED_IMPORT" and fact["target"] == "fastapi.APIRouter" for fact in parse_facts)
assert any(fact["fact_kind"] == "SYMBOL" and fact["target"] == "app" for fact in parse_facts)
assert any(fact["fact_kind"] == "SYMBOL" and fact["target"] == "scope.imported.APIRouter" for fact in parse_facts)
assert any(fact["fact_kind"] == "SYMBOL" and fact["target"] == "scope.namespace.UserOut" for fact in parse_facts)
assert any(fact["fact_kind"] == "SYMBOL" and fact["target"] == "scope.assigned.router" for fact in parse_facts)
assert any(fact["fact_kind"] == "TYPE" and fact["target"] == "pydantic.BaseModel" for fact in parse_facts)
assert any(fact["fact_kind"] == "TYPE" and fact["target"] == "sqlalchemy.orm.DeclarativeBase" for fact in parse_facts)
assert any(fact["fact_kind"] == "SYMBOL" and fact["target"] == "fastapi.APIRouter.get" for fact in parse_facts)
assert any(fact["fact_kind"] == "RESOLVED_CALL" and fact["target"] == "client.get" for fact in parse_facts)
assert any(
    fact["fact_kind"] == "UNKNOWN"
    and fact["target"] == "PytestFixtureInjection"
    and "affected_claim=pytest_fixture_binding" in fact["assumptions"]
    for fact in parse_facts
)
for fact in parse_facts:
    assert fact["origin"]["engine"] == "python"
    assert fact["origin"]["method"] == "cpython_ast"
    assert fact["certainty"] in {"STRUCTURAL", "UNKNOWN"}
    assert fact["evidence"]["path"] == "app.py"
    assert fact["evidence"]["content_hash"] == "sha256:" + "0" * 64
    assert "start_byte" in fact["evidence"]
    assert "end_byte" in fact["evidence"]
assert "from fastapi" not in json.dumps(parse_messages)
assert "@router.get" not in json.dumps(parse_messages)

bad_parse = run_worker(
    {
        "protocol_version": 1,
        "mode": "parse_document",
        "path": "broken.py",
        "content_hash": "sha256:" + "1" * 64,
        "repository_revision": "UNKNOWN",
        "text": "def broken(:\n",
    }
)
assert bad_parse[0]["units"] == []
assert bad_parse[0]["diagnostics"][0]["message"] == "python ast parse failed"
assert bad_parse[0]["facts"] == []

dynamic_messages = run_worker(
    {
        "protocol_version": 1,
        "mode": "parse_document",
        "path": "dynamic.py",
        "content_hash": "sha256:" + "2" * 64,
        "repository_revision": "UNKNOWN",
        "text": """
import importlib

def load(name, obj, method):
    importlib.import_module(name)
    getattr(obj, method)()
    globals()[name]()
""",
    }
)
dynamic_facts = dynamic_messages[0]["facts"]
assert any(
    fact["fact_kind"] == "UNKNOWN"
    and fact["target"] == "DynamicImport"
    and "affected_claim=python_import_resolution" in fact["assumptions"]
    for fact in dynamic_facts
)
assert any(
    fact["fact_kind"] == "UNKNOWN"
    and fact["target"] == "FrameworkMagic"
    and "affected_claim=python_call_target" in fact["assumptions"]
    for fact in dynamic_facts
)
assert "importlib.import_module(name)" not in json.dumps(dynamic_messages)

unsafe_literal_import = run_worker(
    {
        "protocol_version": 1,
        "mode": "parse_document",
        "path": "unsafe_import.py",
        "content_hash": "sha256:" + "3" * 64,
        "repository_revision": "UNKNOWN",
        "text": """
import importlib

def load():
    importlib.import_module("/tmp/secret")
""",
    }
)
serialized_unsafe_import = json.dumps(unsafe_literal_import)
assert "/tmp/secret" not in serialized_unsafe_import
assert any(
    fact["fact_kind"] == "UNKNOWN" and fact["target"] == "DynamicImport"
    for fact in unsafe_literal_import[0]["facts"]
)

config_messages = run_worker(
    {
        "protocol_version": 1,
        "mode": "parse_project_config",
        "path": "pyproject.toml",
        "content_hash": "sha256:" + "4" * 64,
        "repository_revision": "UNKNOWN",
        "text": """
[project]
name = "demo-api"

[tool.pytest.ini_options]
testpaths = ["tests", "../secret"]
pythonpath = ["src", "/tmp/secret"]

[tool.pyright]
include = ["src", "tests"]
extraPaths = ["src/lib", "C:/secret"]

[tool.pyrefly]
project_includes = ["src"]
""",
    }
)
assert len(config_messages) == 1
assert config_messages[0]["mode"] == "parse_project_config"
assert config_messages[0]["path"] == "pyproject.toml"
if sys.version_info >= (3, 11):
    assert config_messages[0]["config"]["project_name"] == "demo-api"
    assert config_messages[0]["config"]["source_roots"] == ["src", "src/lib", "tests"]
    assert config_messages[0]["config"]["tool_sections"] == ["pyrefly", "pyright", "pytest"]
    assert config_messages[0]["unknowns"] == []
else:
    assert config_messages[0]["config"]["source_roots"] == []
    assert config_messages[0]["unknowns"] == [
        {"reason": "MissingDependency", "affected_claim": "python_project_config"}
    ]
serialized_config = json.dumps(config_messages)
assert "../secret" not in serialized_config
assert "/tmp/secret" not in serialized_config
assert "C:/secret" not in serialized_config

bad_config_messages = run_worker(
    {
        "protocol_version": 1,
        "mode": "parse_project_config",
        "path": "pyproject.toml",
        "content_hash": "sha256:" + "5" * 64,
        "repository_revision": "UNKNOWN",
        "text": "[project\nname = 'broken'\n",
    }
)
if sys.version_info >= (3, 11):
    assert bad_config_messages[0]["unknowns"] == [
        {"reason": "MissingProjectConfig", "affected_claim": "python_project_config"}
    ]
else:
    assert bad_config_messages[0]["unknowns"] == [
        {"reason": "MissingDependency", "affected_claim": "python_project_config"}
    ]
assert "[project" not in json.dumps(bad_config_messages)

with tempfile.TemporaryDirectory() as root:
    Path(root, "app.py").write_text(
        """
from fastapi import APIRouter
router = APIRouter()

@router.post("/users")
def create_user():
    return {}
""",
        encoding="utf-8",
    )
    messages = run_worker(valid_request(root))
    assert_end_of_stream(messages)
    assert any(message.get("fact_kind") == "FRAMEWORK_ROLE" for message in messages)
    assert any(
        message.get("fact_kind") == "RESOLVED_IMPORT" and message.get("target") == "fastapi.APIRouter"
        for message in messages
    )
    assert any(
        message.get("fact_kind") == "SYMBOL" and message.get("target") == "fastapi.APIRouter.post"
        for message in messages
    )
    serialized = json.dumps(messages)
    assert root not in serialized
    assert "@router.post" not in serialized

with tempfile.TemporaryDirectory() as root:
    Path(root, "b.py").write_text("def b():\n    return 2\n", encoding="utf-8")
    Path(root, "a.py").write_text("def a():\n    return 1\n", encoding="utf-8")
    first = valid_request(root)
    first["changed_files"] = ["b.py", "a.py"]
    second = valid_request(root)
    second["changed_files"] = ["a.py", "b.py"]
    assert run_worker(first) == run_worker(second)

if hasattr(os, "symlink"):
    with tempfile.TemporaryDirectory() as root, tempfile.TemporaryDirectory() as outside:
        Path(outside, "outside.py").write_text(
            """
from fastapi import APIRouter
router = APIRouter()

@router.get("/outside")
def outside_route():
    return {}
""",
            encoding="utf-8",
        )
        try:
            os.symlink(Path(outside, "outside.py"), Path(root, "link.py"))
        except OSError:
            pass
        else:
            request = valid_request(root)
            request["changed_files"] = ["link.py"]
            messages = run_worker(request)
            assert_end_of_stream(messages)
            assert not any(message.get("fact_kind") == "FRAMEWORK_ROLE" for message in messages)
            assert outside not in json.dumps(messages)

for changed_files in [
    ["/tmp/secret.py"],
    ["../secret.py"],
    ["src/../secret.py"],
    ["./app.py"],
    ["src\\app.py"],
    ["file:///tmp/secret.py"],
    ["C:tmp/source.py"],
    ["app.py", "app.py"],
]:
    with tempfile.TemporaryDirectory() as root:
        request = valid_request(root)
        request["changed_files"] = changed_files
        messages = run_worker(request)
        assert messages[0]["error_code"] == "SEMANTIC_PROTOCOL_VIOLATION"
        assert_end_of_stream(messages)
        assert "/tmp/secret" not in json.dumps(messages)

oversized = run_worker("x" * 1_048_577)
assert oversized[0]["error_code"] == "SEMANTIC_PROTOCOL_VIOLATION"
assert_end_of_stream(oversized)
