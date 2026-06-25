#!/usr/bin/env python3
"""Conservative Python analysis worker for RepoGrammar.

The default stdin contract is the semantic-worker v1 JSON request and NDJSON
response envelope. A private parse-document mode is also used by the Rust
syntax adapter to reuse CPython ast/symtable without hand-written parsing.
"""

from __future__ import annotations

import ast
import hashlib
import json
import re
import sys
from pathlib import Path
from typing import Any

PROTOCOL_VERSION = 1
DEFAULT_REQUEST_ID = "repogrammar-python-semantic-worker"
MAX_STDIN_BYTES = 1_048_576
MAX_PROJECT_ROOT_CHARS = 4096
MAX_CHANGED_FILES = 10_000
MAX_PATH_CHARS = 4096
ROUTE_METHODS = {"delete", "get", "head", "options", "patch", "post", "put"}


def read_stdin() -> str:
    data = sys.stdin.buffer.read(MAX_STDIN_BYTES + 1)
    if len(data) > MAX_STDIN_BYTES:
        raise ValueError("semantic worker request is too large")
    return data.decode("utf-8")


def is_non_blank_string(value: Any) -> bool:
    return isinstance(value, str) and bool(value.strip())


def has_control_or_uri_text(value: str) -> bool:
    return any(ord(ch) < 32 for ch in value) or "://" in value


def looks_like_windows_absolute_path(value: str) -> bool:
    return re.match(r"^[A-Za-z]:[\\/]", value) is not None


def has_windows_drive_prefix(value: str) -> bool:
    return re.match(r"^[A-Za-z]:", value) is not None


def is_absolute_project_root(value: Any) -> bool:
    return (
        is_non_blank_string(value)
        and len(value) <= MAX_PROJECT_ROOT_CHARS
        and not has_control_or_uri_text(value)
        and (value.startswith("/") or looks_like_windows_absolute_path(value))
    )


def is_safe_repo_relative_path(value: Any) -> bool:
    if (
        not is_non_blank_string(value)
        or len(value) > MAX_PATH_CHARS
        or has_control_or_uri_text(value)
        or value.startswith("/")
        or "\\" in value
        or has_windows_drive_prefix(value)
    ):
        return False
    return all(segment and segment not in {".", ".."} for segment in value.split("/"))


def is_strict_content_hash(value: Any) -> bool:
    return isinstance(value, str) and re.fullmatch(r"sha256:[0-9A-Fa-f]{64}", value) is not None


def message(payload: dict[str, Any]) -> None:
    sys.stdout.write(json.dumps(payload, separators=(",", ":"), sort_keys=True))
    sys.stdout.write("\n")


def emit_worker_error(request_id: str, error_code: str, text: str) -> None:
    message(
        {
            "protocol_version": PROTOCOL_VERSION,
            "message_type": "worker_error",
            "request_id": request_id,
            "error_code": error_code,
            "message": text,
            "fallback": {"mode": "syntax_only", "certainty": "UNKNOWN"},
        }
    )
    message(
        {
            "protocol_version": PROTOCOL_VERSION,
            "message_type": "end_of_stream",
            "request_id": request_id,
        }
    )


def byte_line_starts(source: str) -> list[int]:
    starts = [0]
    total = 0
    for line in source.splitlines(keepends=True):
        total += len(line.encode("utf-8"))
        starts.append(total)
    return starts


def byte_offset(starts: list[int], line_number: int | None, column: int | None) -> int:
    if not line_number:
        return 0
    index = max(line_number - 1, 0)
    if index >= len(starts):
        return starts[-1]
    return starts[index] + max(column or 0, 0)


def node_range(starts: list[int], node: ast.AST) -> tuple[int, int]:
    start_line = getattr(node, "lineno", 1)
    start_col = getattr(node, "col_offset", 0)
    decorators = getattr(node, "decorator_list", [])
    if decorators:
        first_decorator = min(decorators, key=lambda decorator: getattr(decorator, "lineno", start_line))
        start_line = getattr(first_decorator, "lineno", start_line)
        start_col = getattr(first_decorator, "col_offset", start_col)
    end_line = getattr(node, "end_lineno", start_line)
    end_col = getattr(node, "end_col_offset", start_col)
    return byte_offset(starts, start_line, start_col), byte_offset(starts, end_line, end_col)


def dotted_name(node: ast.AST) -> str | None:
    if isinstance(node, ast.Call):
        return dotted_name(node.func)
    if isinstance(node, ast.Name):
        return node.id
    if isinstance(node, ast.Attribute):
        base = dotted_name(node.value)
        return f"{base}.{node.attr}" if base else node.attr
    if isinstance(node, ast.Subscript):
        return dotted_name(node.value)
    return None


def slug(value: str) -> str:
    lowered = value.lower()
    return re.sub(r"[^a-z0-9_]+", "_", lowered).strip("_") or "anonymous"


def decorator_names(node: ast.AST) -> list[str]:
    return [name for decorator in getattr(node, "decorator_list", []) if (name := dotted_name(decorator))]


def has_fastapi_route_decorator(node: ast.AST) -> bool:
    for name in decorator_names(node):
        parts = name.split(".")
        if len(parts) >= 2 and parts[-1] in ROUTE_METHODS:
            return True
    return False


def has_pytest_fixture_decorator(node: ast.AST) -> bool:
    return any(name in {"fixture", "pytest.fixture"} for name in decorator_names(node))


def base_names(node: ast.ClassDef) -> list[str]:
    return [name for base in node.bases if (name := dotted_name(base))]


def is_pydantic_model(node: ast.ClassDef) -> bool:
    return any(name.endswith("BaseModel") or name.endswith("BaseSettings") for name in base_names(node))


def is_sqlalchemy_model(node: ast.ClassDef) -> bool:
    if any(name.endswith("DeclarativeBase") or name == "Base" for name in base_names(node)):
        return True
    for item in node.body:
        if isinstance(item, (ast.AnnAssign, ast.Assign)):
            targets = [item.target] if isinstance(item, ast.AnnAssign) else list(item.targets)
            annotation = dotted_name(item.annotation) if isinstance(item, ast.AnnAssign) else None
            value_name = dotted_name(item.value) if isinstance(getattr(item, "value", None), ast.AST) else None
            if annotation and (annotation.endswith("Mapped") or annotation.startswith("Mapped")):
                return True
            if value_name and (value_name.endswith("mapped_column") or value_name.endswith("Column")):
                return True
            if any(isinstance(target, ast.Name) and target.id == "__tablename__" for target in targets):
                return True
    return False


def has_sqlalchemy_repository_call(node: ast.AST) -> bool:
    for child in ast.walk(node):
        if not isinstance(child, ast.Call):
            continue
        name = dotted_name(child.func)
        if not name:
            continue
        if name in {"select", "sqlalchemy.select"}:
            return True
        if name.endswith((".execute", ".commit", ".rollback", ".scalar", ".scalars")):
            return True
    return False


def function_kind(node: ast.FunctionDef | ast.AsyncFunctionDef, class_name: str | None) -> str:
    if has_fastapi_route_decorator(node):
        return "fastapi_route"
    if has_pytest_fixture_decorator(node):
        return "pytest_fixture"
    if node.name.startswith("test_"):
        return "pytest_test"
    if has_sqlalchemy_repository_call(node) and (
        class_name is None or class_name.endswith(("Repository", "Repo", "Service"))
    ):
        return "sqlalchemy_repository_method"
    if class_name is not None:
        return "method"
    if isinstance(node, ast.AsyncFunctionDef):
        return "async_function"
    return "function"


def class_kind(node: ast.ClassDef) -> str:
    if is_pydantic_model(node):
        return "pydantic_model"
    if is_sqlalchemy_model(node):
        return "sqlalchemy_model"
    return "class"


def unit(name: str, kind: str, start: int, end: int, ordinal: int) -> dict[str, Any]:
    return {
        "name": name,
        "kind": kind,
        "start_byte": start,
        "end_byte": end,
        "ordinal": ordinal,
    }


def analyze_source(path: str, source: str) -> tuple[list[dict[str, Any]], list[dict[str, Any]]]:
    starts = byte_line_starts(source)
    units: list[dict[str, Any]] = []
    diagnostics: list[dict[str, Any]] = []

    try:
        tree = ast.parse(source, filename=path)
    except SyntaxError as error:
        diagnostics.append(
            {
                "severity": "error",
                "message": "python ast parse failed",
                "start_byte": byte_offset(starts, error.lineno, error.offset - 1 if error.offset else 0),
                "end_byte": byte_offset(starts, error.lineno, error.offset if error.offset else 0),
            }
        )
        return units, diagnostics

    ordinal = 0
    units.append(unit("module", "module", 0, len(source.encode("utf-8")), ordinal))
    ordinal += 1

    for item in tree.body:
        if isinstance(item, (ast.FunctionDef, ast.AsyncFunctionDef)):
            start, end = node_range(starts, item)
            units.append(unit(item.name, function_kind(item, None), start, end, ordinal))
            ordinal += 1
        elif isinstance(item, ast.ClassDef):
            start, end = node_range(starts, item)
            units.append(unit(item.name, class_kind(item), start, end, ordinal))
            ordinal += 1
            for child in item.body:
                if isinstance(child, (ast.FunctionDef, ast.AsyncFunctionDef)):
                    start, end = node_range(starts, child)
                    units.append(unit(child.name, function_kind(child, item.name), start, end, ordinal))
                    ordinal += 1

    units.sort(key=lambda item: (item["start_byte"], item["end_byte"], item["kind"], item["name"]))
    return units, diagnostics


def parse_document(payload: dict[str, Any]) -> int:
    if set(payload) != {
        "protocol_version",
        "mode",
        "path",
        "content_hash",
        "repository_revision",
        "text",
    }:
        return 2
    if payload.get("protocol_version") != PROTOCOL_VERSION or payload.get("mode") != "parse_document":
        return 2
    if not is_safe_repo_relative_path(payload.get("path")) or not is_strict_content_hash(payload.get("content_hash")):
        return 2
    text = payload.get("text")
    if not isinstance(text, str):
        return 2
    units, diagnostics = analyze_source(payload["path"], text)
    message(
        {
            "protocol_version": PROTOCOL_VERSION,
            "mode": "parse_document",
            "path": payload["path"],
            "units": units,
            "diagnostics": diagnostics,
        }
    )
    return 0


def validate_request(payload: Any) -> bool:
    if not isinstance(payload, dict) or set(payload) != {
        "protocol_version",
        "request_id",
        "project_root",
        "changed_files",
    }:
        return False
    if payload.get("protocol_version") != PROTOCOL_VERSION:
        return False
    if payload.get("request_id") != DEFAULT_REQUEST_ID:
        return False
    if not is_absolute_project_root(payload.get("project_root")):
        return False
    changed_files = payload.get("changed_files")
    if not isinstance(changed_files, list) or len(changed_files) > MAX_CHANGED_FILES:
        return False
    seen: set[str] = set()
    for changed_file in changed_files:
        if not is_safe_repo_relative_path(changed_file) or changed_file in seen:
            return False
        seen.add(changed_file)
    return True


def resolve_under_root(project_root: Path, relative_path: str) -> Path | None:
    try:
        root = project_root.resolve(strict=True)
        target = (root / relative_path).resolve(strict=True)
        target.relative_to(root)
    except (OSError, ValueError):
        return None
    if not target.is_file():
        return None
    return target


def content_hash(path: Path) -> str:
    return f"sha256:{hashlib.sha256(path.read_bytes()).hexdigest()}"


def emit_framework_role_fact(request_id: str, project_root: Path, relative_path: str, unit_data: dict[str, Any]) -> None:
    role_by_kind = {
        "fastapi_route": "framework:fastapi.route",
        "pytest_test": "framework:pytest.test",
        "pytest_fixture": "framework:pytest.fixture",
        "pydantic_model": "framework:pydantic.model",
        "sqlalchemy_model": "framework:sqlalchemy.model",
        "sqlalchemy_repository_method": "framework:sqlalchemy.repository_method",
    }
    role = role_by_kind.get(unit_data["kind"])
    if role is None:
        return
    file_path = resolve_under_root(project_root, relative_path)
    if file_path is None:
        return
    message(
        {
            "protocol_version": PROTOCOL_VERSION,
            "message_type": "fact",
            "request_id": request_id,
            "fact_kind": "FRAMEWORK_ROLE",
            "subject": f"unit:{relative_path}#{unit_data['kind']}:{slug(unit_data['name'])}:{unit_data['start_byte']}-{unit_data['end_byte']}:{unit_data['ordinal']}",
            "target": role,
            "origin": {
                "engine": "python",
                "engine_version": f"{sys.version_info.major}.{sys.version_info.minor}.{sys.version_info.micro}",
                "method": "cpython_ast",
            },
            "certainty": "FRAMEWORK_HEURISTIC",
            "evidence": {
                "code_unit_id": f"unit:{relative_path}#{unit_data['kind']}:{slug(unit_data['name'])}:{unit_data['start_byte']}-{unit_data['end_byte']}:{unit_data['ordinal']}",
                "path": relative_path,
                "content_hash": content_hash(file_path),
                "repository_revision": "UNKNOWN",
                "start_byte": unit_data["start_byte"],
                "end_byte": unit_data["end_byte"],
                "note": f"CPython ast recognized {role}",
            },
            "assumptions": ["binding unresolved without provider"],
        }
    )


def analyze_project(payload: dict[str, Any]) -> int:
    request_id = payload.get("request_id") if isinstance(payload, dict) else DEFAULT_REQUEST_ID
    if not isinstance(request_id, str) or not request_id.strip():
        request_id = DEFAULT_REQUEST_ID
    if not validate_request(payload):
        emit_worker_error(request_id, "SEMANTIC_PROTOCOL_VIOLATION", "semantic worker request is invalid")
        return 0
    project_root = Path(payload["project_root"])
    for relative_path in sorted(payload["changed_files"]):
        if not relative_path.endswith(".py"):
            continue
        file_path = resolve_under_root(project_root, relative_path)
        if file_path is None:
            continue
        try:
            source = file_path.read_text(encoding="utf-8")
        except OSError:
            continue
        units, _diagnostics = analyze_source(relative_path, source)
        for unit_data in units:
            emit_framework_role_fact(request_id, project_root, relative_path, unit_data)
    message({"protocol_version": PROTOCOL_VERSION, "message_type": "end_of_stream", "request_id": request_id})
    return 0


def main() -> int:
    try:
        payload = json.loads(read_stdin())
    except Exception:
        emit_worker_error(DEFAULT_REQUEST_ID, "SEMANTIC_PROTOCOL_VIOLATION", "semantic worker request is invalid")
        return 0
    if isinstance(payload, dict) and payload.get("mode") == "parse_document":
        return parse_document(payload)
    return analyze_project(payload)


if __name__ == "__main__":
    raise SystemExit(main())
