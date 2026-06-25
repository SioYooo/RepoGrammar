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
assert "from fastapi" not in json.dumps(parse_messages)

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
    serialized = json.dumps(messages)
    assert root not in serialized
    assert "@router.post" not in serialized

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
