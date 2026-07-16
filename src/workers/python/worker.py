#!/usr/bin/env python3
"""Conservative Python analysis worker for RepoGrammar.

The default stdin contract is the semantic-worker v1 JSON request and NDJSON
response envelope. A private parse-document mode is also used by the Rust
syntax adapter to reuse CPython ast/symtable without hand-written parsing.
"""

from __future__ import annotations

import ast
from bisect import bisect_left
from collections.abc import Iterator, Mapping
import configparser
import hashlib
import json
import re
import sys
import symtable
from pathlib import Path
from typing import Any

try:
    import tomllib
except ModuleNotFoundError:  # Python < 3.11.
    tomllib = None

PROTOCOL_VERSION = 1
PARSE_DOCUMENT_CONTRACT_REVISION = 1
DEFAULT_REQUEST_ID = "repogrammar-python-semantic-worker"
MAX_STDIN_BYTES = 1_048_576
MAX_PROJECT_ROOT_CHARS = 4096
MAX_CHANGED_FILES = 10_000
MAX_PATH_CHARS = 4096
MAX_SOURCE_BYTES = 1_048_576
MAX_FACTS_PER_FILE = 2_000
MAX_FACT_TARGET_CHARS = 512
MAX_RUST_PARSE_FACT_TARGET_CHARS = 256
MAX_CONFIG_TEXT_BYTES = 1_048_576
# Aggregate byte budget across all changed-file sources read in one
# analyze_project pass. Per-file caps (MAX_SOURCE_BYTES x MAX_CHANGED_FILES)
# still allow ~10 GiB of sources plus their ASTs to be held at once, which can
# OOM-kill the worker mid-stream. This bounds the aggregate so an oversized
# request fails closed with a worker_error instead. It is generous enough to
# cover any real Python codebase's changed-file set.
MAX_TOTAL_SOURCE_BYTES = 512 * 1_048_576
# Recursion-depth guards for the self-recursive AST name helpers. A deeply
# chained attribute/subscript expression (e.g. `a.b.b.b...`) parses fine but
# would overflow Python's recursion limit inside these helpers; past the guard
# the helper abstains (returns None) so one pathological expression cannot abort
# the whole request.
MAX_NAME_RECURSION_DEPTH = 200
PYTHON_IMPORT_GRAPH = "repo_local_python_import_graph"
PYTEST_FIXTURE_GRAPH = "repo_local_pytest_fixture_graph"
ROUTE_METHODS = {"delete", "get", "head", "options", "patch", "post", "put"}
FASTAPI_PARAMETER_MARKERS = {
    "fastapi.Body": ("fastapi_request_body_model", "fastapi.request_body"),
    "fastapi.Cookie": ("fastapi_cookie_param", "fastapi.request_param.cookie"),
    "fastapi.Header": ("fastapi_header_param", "fastapi.request_param.header"),
    "fastapi.Path": ("fastapi_path_param", "fastapi.request_param.path"),
    "fastapi.Query": ("fastapi_query_param", "fastapi.request_param.query"),
}
SQLALCHEMY_SESSION_METHODS = {
    "add",
    "commit",
    "execute",
    "get",
    "rollback",
    "scalar",
    "scalars",
}
SQLALCHEMY_QUERY_METHODS = {
    "execute",
    "scalar",
    "scalars",
}
SQLALCHEMY_CUSTOM_QUERY_WRAPPER_METHODS = {*SQLALCHEMY_QUERY_METHODS, "get"}
PYTEST_BUILTIN_FIXTURES = {
    "cache",
    "capfd",
    "capfdbinary",
    "caplog",
    "capsys",
    "capsysbinary",
    "doctest_namespace",
    "monkeypatch",
    "pytestconfig",
    "record_property",
    "record_testsuite_property",
    "recwarn",
    "request",
    "tmp_path",
    "tmp_path_factory",
}
# Bounded allowlist of distinctive fixtures provided by well-known pytest
# plugins. A test parameter that is not a repo-local, conftest, or built-in
# fixture but exactly matches one of these names is treated as external plugin
# fixture context (like a built-in fixture) instead of a `PytestFixtureInjection`
# UNKNOWN. Kept intentionally conservative: only names that are strongly
# plugin-identifying and unlikely to be defined locally are included, and
# repo-local/conftest definitions still win first. This is sanctioned by the
# UNKNOWN governance rule that plugin fixtures are external context "only when a
# declared allowlist or provider proves the binding"; the resulting fact is
# structural context, never family-support evidence.
PYTEST_PLUGIN_FIXTURES = {
    "aiohttp_client",
    "benchmark",
    "class_mocker",
    "event_loop",
    "event_loop_policy",
    "freezer",
    "httpx_mock",
    "mocker",
    "module_mocker",
    "package_mocker",
    "session_mocker",
    "subtests",
}
SQLALCHEMY_SESSION_TYPES = {
    "sqlalchemy.orm.Session",
    "sqlalchemy.ext.asyncio.AsyncSession",
}
SAFE_NATIVE_DECORATORS = {"classmethod", "property", "staticmethod"}
DYNAMIC_NAMESPACE_FUNCTIONS = {"globals", "locals", "vars"}
DYNAMIC_CALL_ALIAS_TARGETS = {
    "__import__",
    "compile",
    "eval",
    "exec",
    "getattr",
    "importlib.import_module",
    "setattr",
    *DYNAMIC_NAMESPACE_FUNCTIONS,
}
PYDANTIC_VALIDATOR_TARGETS = {
    "pydantic.computed_field",
    "pydantic.field_validator",
    "pydantic.model_validator",
    "pydantic.validator",
}
PYDANTIC_RUNTIME_VALIDATOR_TARGETS = {
    "pydantic.field_validator",
    "pydantic.model_validator",
    "pydantic.validator",
}
PYDANTIC_MODEL_BASES = {
    "pydantic.BaseModel",
    "pydantic.BaseSettings",
    "pydantic_settings.BaseSettings",
}
SQLALCHEMY_MODEL_BASES = {
    "sqlalchemy.orm.DeclarativeBase",
    "sqlalchemy.orm.declarative_base",
}
SQLALCHEMY_MAPPED_TYPES = {
    "sqlalchemy.orm.Mapped",
}
SQLALCHEMY_MAPPED_CALLS = {
    "sqlalchemy.orm.mapped_column",
}
SQLALCHEMY_EVENT_LISTENER_CALLS = {
    "sqlalchemy.event.listen",
    "sqlalchemy.event.listens_for",
}
# Bounded preview anchors (ADR-0019 D3/D4): Django, Flask, stdlib unittest,
# click/typer, and Celery. Framework identity always requires the base or
# decorator receiver to resolve to an exact framework import binding, exactly
# like FastAPI/SQLAlchemy above; name lookalikes without the import stay
# UNKNOWN. These previews do not change the ADR-0011 v0.1 focus statement.
DJANGO_MODEL_BASES = {"django.db.models.Model"}
DJANGO_TEST_BASES = {
    "django.test.TestCase",
    "django.test.SimpleTestCase",
    "django.test.TransactionTestCase",
}
DJANGO_URL_CALLS = {"django.urls.path", "django.urls.re_path"}
DJANGO_URL_INCLUDE = "django.urls.include"
DJANGO_FIELD_MODULE_PREFIX = "django.db.models."
UNITTEST_TEST_BASES = {"unittest.TestCase"}
UNITTEST_PATCH_TARGET = "unittest.mock.patch"
FLASK_APP_RECEIVER_TYPES = {"flask.Flask", "flask.Blueprint"}
FLASK_ROUTE_ATTRS = {"route", "get", "post", "put", "delete", "patch"}
FLASK_METHOD_ATTRS = {"get", "post", "put", "delete", "patch"}
CLICK_COMMAND_DECORATORS = {"click.command", "click.group"}
CLICK_PARAM_DECORATORS = {"click.option", "click.argument"}
TYPER_APP_RECEIVER_TYPE = "typer.Typer"
CELERY_APP_RECEIVER_TYPE = "celery.Celery"
CELERY_SHARED_TASK = "celery.shared_task"
CELERY_RUNTIME_METHODS = {"delay", "apply_async"}
CELERY_SEND_TASK_ATTR = "send_task"


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


def is_safe_fact_target(value: Any) -> bool:
    return (
        is_non_blank_string(value)
        and len(value) <= MAX_FACT_TARGET_CHARS
        and not has_control_or_uri_text(value)
        and not has_windows_drive_prefix(value)
        and re.fullmatch(r"[A-Za-z0-9_.-]+", value) is not None
    )


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


def emit_parse_document_contract_mismatch() -> None:
    message(
        {
            "protocol_version": PROTOCOL_VERSION,
            "contract_revision": PARSE_DOCUMENT_CONTRACT_REVISION,
            "mode": "parse_document",
            "error_code": "PYTHON_FRONTEND_CONTRACT_MISMATCH",
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
    cached = getattr(node, "_repogrammar_byte_range", None)
    if cached is not None:
        return cached
    start_line = getattr(node, "lineno", 1)
    start_col = getattr(node, "col_offset", 0)
    decorators = getattr(node, "decorator_list", [])
    if decorators:
        first_decorator = min(decorators, key=lambda decorator: getattr(decorator, "lineno", start_line))
        start_line = getattr(first_decorator, "lineno", start_line)
        start_col = getattr(first_decorator, "col_offset", start_col)
    end_line = getattr(node, "end_lineno", start_line)
    end_col = getattr(node, "end_col_offset", start_col)
    result = (
        byte_offset(starts, start_line, start_col),
        byte_offset(starts, end_line, end_col),
    )
    # A worker invocation parses one immutable source string into one AST. Range
    # lookup is extremely hot for large modules, so cache it on the node rather
    # than repeatedly re-walking decorators and line offsets. `ast.walk` follows
    # `_fields` only, so this private attribute cannot change traversal or output.
    setattr(node, "_repogrammar_byte_range", result)
    return result


def unit_id(path: str, unit_data: dict[str, Any]) -> str:
    return (
        f"unit:{path}#{unit_data['kind']}:{slug(unit_data['name'])}:"
        f"{unit_data['start_byte']}-{unit_data['end_byte']}:{unit_data['ordinal']}"
    )


def dotted_name(node: ast.AST, _depth: int = 0) -> str | None:
    if _depth > MAX_NAME_RECURSION_DEPTH:
        return None
    if isinstance(node, ast.Call):
        return dotted_name(node.func, _depth + 1)
    if isinstance(node, ast.Name):
        return node.id
    if isinstance(node, ast.Attribute):
        base = dotted_name(node.value, _depth + 1)
        return f"{base}.{node.attr}" if base else node.attr
    if isinstance(node, ast.Subscript):
        return dotted_name(node.value, _depth + 1)
    return None


def static_type_name(node: ast.AST, _depth: int = 0) -> str | None:
    if _depth > MAX_NAME_RECURSION_DEPTH:
        return None
    if name := static_reference_name(node):
        return name
    if isinstance(node, ast.Subscript):
        return static_type_name(node.slice, _depth + 1)
    return None


def static_reference_name(node: ast.AST) -> str | None:
    if isinstance(node, (ast.Name, ast.Attribute)):
        return dotted_name(node)
    return None


def slug(value: str) -> str:
    lowered = value.lower()
    return re.sub(r"[^a-z0-9_]+", "_", lowered).strip("_") or "anonymous"


def is_python_identifier(value: str) -> bool:
    return re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", value) is not None


def module_name_from_path(path: str) -> str | None:
    if not path.endswith(".py"):
        return None
    without_suffix = path[:-3]
    parts = without_suffix.split("/")
    if parts[-1] == "__init__":
        parts = parts[:-1]
    if not parts or not all(is_python_identifier(part) for part in parts):
        return None
    return ".".join(parts)


def module_names_for_path(path: str, source_roots: list[str]) -> list[str]:
    candidates: list[str] = []
    for root in source_roots:
        prefix = f"{root}/"
        if path.startswith(prefix):
            candidates.append(path[len(prefix) :])
    if not candidates:
        candidates.append(path)
    result: list[str] = []
    for candidate in candidates:
        module_name = module_name_from_path(candidate)
        if module_name and module_name not in result:
            result.append(module_name)
    return result


def build_module_index(paths: list[str], source_roots: list[str]) -> dict[str, list[str]]:
    modules: dict[str, list[str]] = {}
    for path in sorted(paths):
        if not path.endswith(".py"):
            continue
        for module_name in module_names_for_path(path, source_roots):
            modules.setdefault(module_name, []).append(path)
    return {module_name: sorted(module_paths) for module_name, module_paths in sorted(modules.items())}


def literal_str_list(node: ast.AST) -> list[str] | None:
    if not isinstance(node, (ast.List, ast.Tuple)):
        return None
    values: list[str] = []
    for item in node.elts:
        if isinstance(item, ast.Constant) and isinstance(item.value, str) and is_python_identifier(item.value):
            values.append(item.value)
        else:
            return None
    return values


def module_direct_symbols(tree: ast.Module) -> dict[str, str]:
    symbols: dict[str, str] = {}
    for node in tree.body:
        if isinstance(node, ast.ClassDef):
            symbols.setdefault(node.name, "TYPE")
        elif isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
            symbols.setdefault(node.name, "SYMBOL")
        elif isinstance(node, (ast.Assign, ast.AnnAssign)):
            for name in assignment_target_names(node):
                if name != "__all__":
                    symbols.setdefault(name, "SYMBOL")
    return symbols


def module_literal_all(tree: ast.Module) -> set[str] | None:
    literal: set[str] | None = None
    for node in tree.body:
        value: ast.AST | None = None
        if isinstance(node, ast.Assign):
            if not any(isinstance(target, ast.Name) and target.id == "__all__" for target in node.targets):
                continue
            value = node.value
        elif isinstance(node, ast.AnnAssign) and isinstance(node.target, ast.Name) and node.target.id == "__all__":
            value = node.value
        if value is None:
            continue
        values = literal_str_list(value)
        if values is None:
            return None
        literal = set(values)
    return literal


def build_module_symbol_index(
    file_records: list[tuple[str, str, str]],
    source_roots: list[str],
) -> tuple[dict[str, dict[str, tuple[str, str]]], dict[str, set[str]]]:
    parsed_modules: dict[str, tuple[str, ast.Module]] = {}
    symbol_index: dict[str, dict[str, tuple[str, str]]] = {}
    literal_all_index: dict[str, set[str]] = {}
    for path, source, _file_hash in sorted(file_records):
        if not path.endswith(".py"):
            continue
        try:
            tree = ast.parse(source, filename=path)
        except SyntaxError:
            continue
        for module_name in module_names_for_path(path, source_roots):
            parsed_modules[module_name] = (path, tree)
            direct_symbols = module_direct_symbols(tree)
            if direct_symbols:
                symbol_index[module_name] = {
                    name: (kind, f"{module_name}.{name}") for name, kind in sorted(direct_symbols.items())
                }
            literal_all = module_literal_all(tree)
            if literal_all is not None:
                literal_all_index[module_name] = literal_all

    for module_name, (path, tree) in sorted(parsed_modules.items()):
        if not (path == "__init__.py" or path.endswith("/__init__.py")):
            continue
        package_symbols = symbol_index.setdefault(module_name, {})
        for node in tree.body:
            if not isinstance(node, ast.ImportFrom):
                continue
            imported_module = (
                relative_import_base([module_name], path, node.level, node.module)
                if node.level
                else node.module
            )
            if imported_module is None:
                continue
            for alias in node.names:
                if alias.name == "*":
                    for exported in sorted(literal_all_index.get(imported_module, set())):
                        resolved = symbol_index.get(imported_module, {}).get(exported)
                        if resolved is not None:
                            package_symbols.setdefault(exported, resolved)
                    continue
                local_name = alias.asname or alias.name
                resolved = symbol_index.get(imported_module, {}).get(alias.name)
                if resolved is not None:
                    package_symbols.setdefault(local_name, resolved)

    return symbol_index, literal_all_index


def parent_path(path: str) -> str:
    parts = path.split("/")
    return "/".join(parts[:-1])


def infer_source_roots(paths: list[str]) -> list[str]:
    package_dirs = {parent_path(path) for path in paths if path.endswith("/__init__.py") or path == "__init__.py"}
    roots: set[str] = set()
    for package_dir in sorted(package_dirs):
        if not package_dir:
            continue
        top = package_dir
        while (parent := parent_path(top)) in package_dirs:
            top = parent
        root = parent_path(top)
        if root and is_safe_repo_relative_path(root):
            roots.add(root)
    return sorted(roots)


def safe_path_list(value: Any, *, require_python: bool = False) -> list[str] | None:
    if value is None:
        return []
    if not isinstance(value, list) or len(value) > MAX_CHANGED_FILES:
        return None
    result: list[str] = []
    seen: set[str] = set()
    for item in value:
        if not is_safe_repo_relative_path(item) or item in seen:
            return None
        if require_python and not item.endswith(".py"):
            return None
        seen.add(item)
        result.append(item)
    return sorted(result)


def decorator_names(node: ast.AST) -> list[str]:
    return [name for decorator in getattr(node, "decorator_list", []) if (name := dotted_name(decorator))]


def canonical_decorator_names(
    node: ast.AST,
    aliases: dict[str, str] | None = None,
    assignments: dict[str, str] | None = None,
) -> list[str]:
    aliases = aliases or {}
    assignments = assignments or {}
    return [canonical_name(name, aliases, assignments) for name in decorator_names(node)]


def has_fastapi_route_decorator(
    node: ast.AST,
    aliases: dict[str, str] | None = None,
    assignments: dict[str, str] | None = None,
) -> bool:
    for name in canonical_decorator_names(node, aliases, assignments):
        if decorator_anchor_kind(name) == "fastapi_route_decorator":
            return True
    return False


def has_pytest_fixture_decorator(
    node: ast.AST,
    aliases: dict[str, str] | None = None,
    assignments: dict[str, str] | None = None,
) -> bool:
    return any(
        name == "pytest.fixture"
        for name in canonical_decorator_names(node, aliases, assignments)
    )


def pytest_fixture_binding_names(
    node: ast.FunctionDef | ast.AsyncFunctionDef,
    aliases: dict[str, str] | None = None,
    assignments: dict[str, str] | None = None,
) -> tuple[list[str], bool]:
    aliases = aliases or {}
    assignments = assignments or {}
    names: set[str] = set()
    has_unknown_name = False
    for decorator in getattr(node, "decorator_list", []):
        raw_name = dotted_name(decorator)
        if raw_name is None or canonical_name(raw_name, aliases, assignments) != "pytest.fixture":
            continue
        explicit_name = False
        if isinstance(decorator, ast.Call):
            for keyword in decorator.keywords:
                if keyword.arg != "name":
                    continue
                explicit_name = True
                if isinstance(keyword.value, ast.Constant) and keyword.value.value is None:
                    names.add(node.name)
                elif isinstance(keyword.value, ast.Constant) and isinstance(keyword.value.value, str):
                    candidate = keyword.value.value
                    if is_safe_fact_target(f"pytest.fixture.{candidate}"):
                        names.add(candidate)
                    else:
                        has_unknown_name = True
                else:
                    has_unknown_name = True
        if not explicit_name:
            names.add(node.name)
    return sorted(names), has_unknown_name


def pytest_fixture_names_from_tree(
    tree: ast.Module,
    aliases: dict[str, str] | None = None,
    assignments: dict[str, str] | None = None,
) -> set[str]:
    return set(pytest_fixture_name_counts_from_tree(tree, aliases, assignments))


def pytest_fixture_name_counts_from_tree(
    tree: ast.Module,
    aliases: dict[str, str] | None = None,
    assignments: dict[str, str] | None = None,
) -> dict[str, int]:
    counts: dict[str, int] = {}
    for item in tree.body:
        if isinstance(item, (ast.FunctionDef, ast.AsyncFunctionDef)):
            names, _has_unknown_name = pytest_fixture_binding_names(item, aliases, assignments)
            for name in names:
                counts[name] = counts.get(name, 0) + 1
    return counts


def base_names(node: ast.ClassDef) -> list[str]:
    return [name for base in node.bases if (name := dotted_name(base))]


def is_pydantic_model(node: ast.ClassDef, aliases: dict[str, str] | None = None) -> bool:
    aliases = aliases or {}
    return any(
        canonical_name(name, aliases, {}) in PYDANTIC_MODEL_BASES
        for name in base_names(node)
    )


def class_has_pydantic_member_signal(node: ast.ClassDef, aliases: dict[str, str]) -> bool:
    for item in node.body:
        if isinstance(item, ast.AnnAssign):
            if isinstance(item.target, ast.Name) and item.target.id == "model_config":
                return True
            if isinstance(item.value, ast.Call):
                value_name = dotted_name(item.value.func)
                if value_name and canonical_name(value_name, aliases, {}) == "pydantic.Field":
                    return True
        elif isinstance(item, ast.Assign):
            if any(isinstance(target, ast.Name) and target.id == "model_config" for target in item.targets):
                return True
        elif isinstance(item, ast.ClassDef) and item.name == "Config":
            return True
        elif isinstance(item, (ast.FunctionDef, ast.AsyncFunctionDef)):
            for decorator in item.decorator_list:
                name = dotted_name(decorator)
                if name and canonical_name(name, aliases, {}) in PYDANTIC_VALIDATOR_TARGETS:
                    return True
    return False


def class_has_sqlalchemy_member_signal(
    node: ast.ClassDef,
    aliases: dict[str, str],
    assignments: dict[str, str],
) -> bool:
    for item in node.body:
        if not isinstance(item, (ast.AnnAssign, ast.Assign)):
            continue
        targets = [item.target] if isinstance(item, ast.AnnAssign) else list(item.targets)
        if any(isinstance(target, ast.Name) and target.id == "__tablename__" for target in targets):
            return True
        annotation = dotted_name(item.annotation) if isinstance(item, ast.AnnAssign) else None
        value_name = dotted_name(item.value) if isinstance(getattr(item, "value", None), ast.AST) else None
        if annotation and canonical_name(annotation, aliases, {}) in SQLALCHEMY_MAPPED_TYPES:
            return True
        if value_name and canonical_name(value_name, aliases, assignments) in {
            *SQLALCHEMY_MAPPED_CALLS,
            "sqlalchemy.orm.relationship",
        }:
            return True
    return False


def external_framework_base_nodes(
    node: ast.ClassDef,
    aliases: dict[str, str],
    assignments: dict[str, str],
    framework_prefixes: tuple[str, ...],
    exact_framework_bases: set[str],
) -> list[ast.AST]:
    external_bases: list[ast.AST] = []
    for base in node.bases:
        name = dotted_name(base)
        if not name:
            continue
        canonical = canonical_name(name, aliases, assignments)
        if canonical in exact_framework_bases or canonical.startswith(framework_prefixes):
            continue
        root = name.split(".", 1)[0]
        if root in aliases:
            external_bases.append(base)
    return external_bases


def is_sqlalchemy_model(
    node: ast.ClassDef,
    aliases: dict[str, str] | None = None,
    assignments: dict[str, str] | None = None,
) -> bool:
    aliases = aliases or {}
    assignments = assignments or {}
    if any(
        canonical_name(name, aliases, assignments) in SQLALCHEMY_MODEL_BASES
        for name in base_names(node)
    ):
        return True
    for item in node.body:
        if isinstance(item, (ast.AnnAssign, ast.Assign)):
            targets = [item.target] if isinstance(item, ast.AnnAssign) else list(item.targets)
            annotation = dotted_name(item.annotation) if isinstance(item, ast.AnnAssign) else None
            value_name = dotted_name(item.value) if isinstance(getattr(item, "value", None), ast.AST) else None
            if annotation and canonical_name(annotation, aliases, {}) in SQLALCHEMY_MAPPED_TYPES:
                return True
            if value_name and canonical_name(value_name, aliases, {}) in SQLALCHEMY_MAPPED_CALLS:
                return True
    return False


def has_sqlalchemy_repository_call(node: ast.AST, aliases: dict[str, str] | None = None) -> bool:
    aliases = aliases or {}
    parameter_roles = (
        collect_parameter_roles(node, aliases)
        if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef))
        else {}
    )
    for child in ast.walk(node):
        if not isinstance(child, ast.Call):
            continue
        name = dotted_name(child.func)
        if not name:
            continue
        if name in {"select", "sqlalchemy.select"}:
            return True
        if name.endswith(".get"):
            receiver = name.rsplit(".", 1)[0]
            if receiver in parameter_roles:
                return True
        if name.endswith((".execute", ".commit", ".rollback", ".scalar", ".scalars")):
            return True
    return False


def is_django_model(
    node: ast.ClassDef,
    aliases: dict[str, str] | None = None,
    assignments: dict[str, str] | None = None,
) -> bool:
    aliases = aliases or {}
    assignments = assignments or {}
    return any(
        canonical_name(name, aliases, assignments) in DJANGO_MODEL_BASES
        for name in base_names(node)
    )


def is_django_test(
    node: ast.ClassDef,
    aliases: dict[str, str] | None = None,
    assignments: dict[str, str] | None = None,
) -> bool:
    aliases = aliases or {}
    assignments = assignments or {}
    return any(
        canonical_name(name, aliases, assignments) in DJANGO_TEST_BASES
        for name in base_names(node)
    )


def is_unittest_test_case(
    node: ast.ClassDef,
    aliases: dict[str, str] | None = None,
    assignments: dict[str, str] | None = None,
) -> bool:
    aliases = aliases or {}
    assignments = assignments or {}
    return any(
        canonical_name(name, aliases, assignments) in UNITTEST_TEST_BASES
        for name in base_names(node)
    )


def class_has_django_model_lookalike_base(
    node: ast.ClassDef,
    aliases: dict[str, str],
    assignments: dict[str, str],
) -> bool:
    for name in base_names(node):
        canonical = canonical_name(name, aliases, assignments)
        if canonical == "models.Model" or canonical.endswith(".models.Model"):
            return True
    return False


def enclosing_class_framework(
    node: ast.ClassDef,
    aliases: dict[str, str] | None = None,
    assignments: dict[str, str] | None = None,
) -> str | None:
    if is_unittest_test_case(node, aliases, assignments):
        return "unittest"
    if is_django_test(node, aliases, assignments):
        return "django_test"
    return None


def has_flask_route_decorator(
    node: ast.AST,
    aliases: dict[str, str] | None = None,
    assignments: dict[str, str] | None = None,
) -> bool:
    for name in canonical_decorator_names(node, aliases, assignments):
        parts = name.split(".")
        if (
            len(parts) >= 3
            and ".".join(parts[:-1]) in FLASK_APP_RECEIVER_TYPES
            and parts[-1] in FLASK_ROUTE_ATTRS
        ):
            return True
    return False


def has_click_command_decorator(
    node: ast.AST,
    aliases: dict[str, str] | None = None,
    assignments: dict[str, str] | None = None,
) -> bool:
    return any(
        name in CLICK_COMMAND_DECORATORS
        for name in canonical_decorator_names(node, aliases, assignments)
    )


def has_typer_command_decorator(
    node: ast.AST,
    aliases: dict[str, str] | None = None,
    assignments: dict[str, str] | None = None,
) -> bool:
    for name in canonical_decorator_names(node, aliases, assignments):
        parts = name.split(".")
        if (
            len(parts) >= 3
            and ".".join(parts[:-1]) == TYPER_APP_RECEIVER_TYPE
            and parts[-1] == "command"
        ):
            return True
    return False


def has_celery_task_decorator(
    node: ast.AST,
    aliases: dict[str, str] | None = None,
    assignments: dict[str, str] | None = None,
) -> bool:
    for name in canonical_decorator_names(node, aliases, assignments):
        if name == CELERY_SHARED_TASK:
            return True
        parts = name.split(".")
        if (
            len(parts) >= 3
            and ".".join(parts[:-1]) == CELERY_APP_RECEIVER_TYPE
            and parts[-1] == "task"
        ):
            return True
    return False


def count_shape_bucket(count: int) -> str:
    if count <= 3:
        return "1_to_3"
    if count <= 8:
        return "4_to_8"
    return "9_plus"


def function_kind(
    node: ast.FunctionDef | ast.AsyncFunctionDef,
    class_name: str | None,
    aliases: dict[str, str] | None = None,
    assignments: dict[str, str] | None = None,
    class_framework: str | None = None,
) -> str:
    if has_fastapi_route_decorator(node, aliases, assignments):
        return "fastapi_route"
    if has_flask_route_decorator(node, aliases, assignments):
        return "flask_route"
    if has_click_command_decorator(node, aliases, assignments):
        return "click_command"
    if has_typer_command_decorator(node, aliases, assignments):
        return "typer_command"
    if has_celery_task_decorator(node, aliases, assignments):
        return "celery_task"
    if has_pytest_fixture_decorator(node, aliases, assignments):
        return "pytest_fixture"
    if node.name.startswith("test_"):
        if class_framework == "unittest":
            return "unittest_test_method"
        if class_framework == "django_test":
            return "method"
        return "pytest_test"
    if has_sqlalchemy_repository_call(node, aliases) and (
        class_name is None or class_name.endswith(("Repository", "Repo", "Service"))
    ):
        return "sqlalchemy_repository_method"
    if class_name is not None:
        return "method"
    if isinstance(node, ast.AsyncFunctionDef):
        return "async_function"
    return "function"


def class_kind(
    node: ast.ClassDef,
    aliases: dict[str, str] | None = None,
    assignments: dict[str, str] | None = None,
) -> str:
    if is_pydantic_model(node, aliases):
        return "pydantic_model"
    if is_sqlalchemy_model(node, aliases, assignments):
        return "sqlalchemy_model"
    if is_django_model(node, aliases, assignments):
        return "django_model"
    if is_django_test(node, aliases, assignments):
        return "django_test"
    return "class"


def unit(name: str, kind: str, start: int, end: int, ordinal: int) -> dict[str, Any]:
    return {
        "name": name,
        "kind": kind,
        "start_byte": start,
        "end_byte": end,
        "ordinal": ordinal,
    }


def canonical_name(name: str, aliases: dict[str, str], assignments: dict[str, str]) -> str:
    parts = name.split(".")
    if not parts:
        return name
    if parts[0] in assignments:
        return ".".join([assignments[parts[0]], *parts[1:]])
    if parts[0] in aliases:
        return ".".join([aliases[parts[0]], *parts[1:]])
    return name


def is_constructor_like_target(value: str) -> bool:
    if not is_safe_fact_target(value):
        return False
    leaf = value.rsplit(".", 1)[-1]
    return "." in value and is_python_identifier(leaf) and leaf[:1].isupper()


def is_application_call_target(value: str) -> bool:
    if "." not in value or not is_safe_fact_target(value):
        return False
    root = value.split(".", 1)[0]
    if root in {"self", "cls"} or not root[:1].islower():
        return False
    if value.count(".") < 2:
        return False
    framework_prefixes = (
        "fastapi.",
        "importlib.",
        "pydantic.",
        "pydantic_settings.",
        "pytest.",
        "sqlalchemy.",
        "sys.",
    )
    return not value.startswith(framework_prefixes)


def evidence(
    path: str,
    content_hash_value: str,
    repository_revision: str,
    subject_unit_id: str,
    start: int,
    end: int,
    note: str,
) -> dict[str, Any]:
    return {
        "code_unit_id": subject_unit_id,
        "path": path,
        "content_hash": content_hash_value,
        "repository_revision": repository_revision,
        "start_byte": start,
        "end_byte": end,
        "note": note,
    }


def fact(
    *,
    kind: str,
    subject: str,
    target: str | None,
    certainty: str,
    path: str,
    content_hash_value: str,
    repository_revision: str,
    subject_unit_id: str,
    start: int,
    end: int,
    note: str,
    assumptions: list[str],
) -> dict[str, Any]:
    return {
        "fact_kind": kind,
        "subject": subject,
        "target": target,
        "origin": {
            "engine": "python",
            "engine_version": f"{sys.version_info.major}.{sys.version_info.minor}.{sys.version_info.micro}",
            "method": "cpython_ast",
        },
        "certainty": certainty,
        "evidence": evidence(path, content_hash_value, repository_revision, subject_unit_id, start, end, note),
        "assumptions": assumptions,
    }


def structural_fact(
    *,
    kind: str,
    subject_unit_id: str,
    target: str,
    path: str,
    content_hash_value: str,
    repository_revision: str,
    start: int,
    end: int,
    anchor_kind: str,
) -> dict[str, Any]:
    return fact(
        kind=kind,
        subject=subject_unit_id,
        target=target,
        certainty="STRUCTURAL",
        path=path,
        content_hash_value=content_hash_value,
        repository_revision=repository_revision,
        subject_unit_id=subject_unit_id,
        start=start,
        end=end,
        note=f"CPython ast structural {anchor_kind}",
        assumptions=[f"python_anchor_kind={anchor_kind}", "binding unresolved without provider"],
    )


def dataflow_derived_fact(
    *,
    kind: str,
    subject_unit_id: str,
    target: str,
    path: str,
    content_hash_value: str,
    repository_revision: str,
    start: int,
    end: int,
    anchor_kind: str,
    derived_from: str,
) -> dict[str, Any]:
    return fact(
        kind=kind,
        subject=subject_unit_id,
        target=target,
        certainty="DATAFLOW_DERIVED",
        path=path,
        content_hash_value=content_hash_value,
        repository_revision=repository_revision,
        subject_unit_id=subject_unit_id,
        start=start,
        end=end,
        note=f"CPython ast {derived_from} {anchor_kind}",
        assumptions=[
            f"python_anchor_kind={anchor_kind}",
            "provider_resolved=false",
            f"derived_from={derived_from}",
        ],
    )


def unknown_fact(
    *,
    subject_unit_id: str,
    reason_code: str,
    affected_claim: str,
    path: str,
    content_hash_value: str,
    repository_revision: str,
    start: int,
    end: int,
) -> dict[str, Any]:
    return fact(
        kind="UNKNOWN",
        subject=subject_unit_id,
        target=reason_code,
        certainty="UNKNOWN",
        path=path,
        content_hash_value=content_hash_value,
        repository_revision=repository_revision,
        subject_unit_id=subject_unit_id,
        start=start,
        end=end,
        note=f"typed UNKNOWN {reason_code} for {affected_claim}",
        assumptions=[f"reason_code={reason_code}", f"affected_claim={affected_claim}"],
    )


def import_fact(
    *,
    subject_unit_id: str,
    target: str,
    path: str,
    content_hash_value: str,
    repository_revision: str,
    start: int,
    end: int,
    anchor_kind: str,
) -> dict[str, Any]:
    if anchor_kind == "repo_local_import_binding":
        return dataflow_derived_fact(
            kind="RESOLVED_IMPORT",
            subject_unit_id=subject_unit_id,
            target=target,
            path=path,
            content_hash_value=content_hash_value,
            repository_revision=repository_revision,
            start=start,
            end=end,
            anchor_kind=anchor_kind,
            derived_from=PYTHON_IMPORT_GRAPH,
        )
    return structural_fact(
        kind="RESOLVED_IMPORT",
        subject_unit_id=subject_unit_id,
        target=target,
        path=path,
        content_hash_value=content_hash_value,
        repository_revision=repository_revision,
        start=start,
        end=end,
        anchor_kind=anchor_kind,
    )


def repo_local_symbol_fact(
    *,
    subject_unit_id: str,
    target: str,
    symbol_kind: str,
    path: str,
    content_hash_value: str,
    repository_revision: str,
    start: int,
    end: int,
) -> dict[str, Any]:
    return dataflow_derived_fact(
        kind=symbol_kind,
        subject_unit_id=subject_unit_id,
        target=target,
        path=path,
        content_hash_value=content_hash_value,
        repository_revision=repository_revision,
        start=start,
        end=end,
        anchor_kind="repo_local_import_symbol",
        derived_from=PYTHON_IMPORT_GRAPH,
    )


def unresolved_import_fact(
    *,
    subject_unit_id: str,
    path: str,
    content_hash_value: str,
    repository_revision: str,
    start: int,
    end: int,
) -> dict[str, Any]:
    return unknown_fact(
        subject_unit_id=subject_unit_id,
        reason_code="UnresolvedImport",
        affected_claim="python_import_resolution",
        path=path,
        content_hash_value=content_hash_value,
        repository_revision=repository_revision,
        start=start,
        end=end,
    )


def repo_local_module_resolution(target: str, module_index: dict[str, list[str]] | None) -> str:
    if module_index is None:
        return "missing"
    matches = module_index.get(target, [])
    if len(matches) == 1:
        return "resolved"
    if len(matches) > 1:
        return "ambiguous"
    return "missing"


def repo_local_prefix_exists(target: str, module_index: dict[str, list[str]] | None) -> bool:
    if module_index is None:
        return False
    parts = target.split(".")
    for end in range(len(parts) - 1, 0, -1):
        if ".".join(parts[:end]) in module_index:
            return True
    return False


def repo_local_symbol_resolution(
    module_name: str,
    symbol_name: str,
    module_symbols: dict[str, dict[str, tuple[str, str]]] | None,
) -> tuple[str, str] | None:
    if module_symbols is None:
        return None
    return module_symbols.get(module_name, {}).get(symbol_name)


def relative_import_base(current_modules: list[str], path: str, level: int, module: str | None) -> str | None:
    if not current_modules or level < 1:
        return None
    bases: list[str] = []
    is_package_init = path.endswith("/__init__.py") or path == "__init__.py"
    for current_module in current_modules:
        parts = current_module.split(".")
        package_parts = parts if is_package_init else parts[:-1]
        if level > 1:
            if len(package_parts) < level - 1:
                continue
            package_parts = package_parts[: -(level - 1)]
        if module:
            package_parts = [*package_parts, *module.split(".")]
        base = ".".join(package_parts)
        if base not in bases:
            bases.append(base)
    return bases[0] if len(bases) == 1 else None


def collect_import_aliases(
    tree: ast.Module,
    starts: list[int],
    path: str,
    content_hash_value: str,
    repository_revision: str,
    module_unit_id: str,
    module_index: dict[str, list[str]] | None = None,
    source_roots: list[str] | None = None,
    module_symbols: dict[str, dict[str, tuple[str, str]]] | None = None,
    module_all_names: dict[str, set[str]] | None = None,
) -> tuple[dict[str, str], list[dict[str, Any]]]:
    aliases: dict[str, str] = {}
    facts: list[dict[str, Any]] = []
    imports = [node for node in tree.body if isinstance(node, (ast.Import, ast.ImportFrom))]
    imports.sort(key=lambda node: node_range(starts, node))
    for node in imports:
        start, end = node_range(starts, node)
        if isinstance(node, ast.Import):
            for alias in node.names:
                local = alias.asname or alias.name.split(".")[0]
                repo_resolution = repo_local_module_resolution(alias.name, module_index)
                if repo_resolution == "ambiguous":
                    facts.append(
                        unresolved_import_fact(
                            subject_unit_id=module_unit_id,
                            path=path,
                            content_hash_value=content_hash_value,
                            repository_revision=repository_revision,
                            start=start,
                            end=end,
                        )
                    )
                    continue
                if repo_resolution == "missing" and repo_local_prefix_exists(alias.name, module_index):
                    facts.append(
                        unresolved_import_fact(
                            subject_unit_id=module_unit_id,
                            path=path,
                            content_hash_value=content_hash_value,
                            repository_revision=repository_revision,
                            start=start,
                            end=end,
                        )
                    )
                    continue
                aliases[local] = alias.name
                facts.append(
                    import_fact(
                        subject_unit_id=module_unit_id,
                        target=alias.name,
                        path=path,
                        content_hash_value=content_hash_value,
                        repository_revision=repository_revision,
                        start=start,
                        end=end,
                        anchor_kind="repo_local_import_binding"
                        if repo_resolution == "resolved"
                        else "import_binding",
                    )
                )
        elif node.level:
            current_modules = module_names_for_path(path, source_roots or [])
            relative_base = relative_import_base(current_modules, path, node.level, node.module)
            if relative_base is None or not node.names:
                facts.append(
                    unresolved_import_fact(
                        subject_unit_id=module_unit_id,
                        path=path,
                        content_hash_value=content_hash_value,
                        repository_revision=repository_revision,
                        start=start,
                        end=end,
                    )
                )
                continue
            for alias in node.names:
                if alias.name == "*":
                    star_names = sorted((module_all_names or {}).get(relative_base, set()))
                    if not star_names:
                        facts.append(
                            unresolved_import_fact(
                                subject_unit_id=module_unit_id,
                                path=path,
                                content_hash_value=content_hash_value,
                                repository_revision=repository_revision,
                                start=start,
                                end=end,
                            )
                        )
                        continue
                    for star_name in star_names:
                        resolved_symbol = repo_local_symbol_resolution(relative_base, star_name, module_symbols)
                        if resolved_symbol is None:
                            facts.append(
                                unresolved_import_fact(
                                    subject_unit_id=module_unit_id,
                                    path=path,
                                    content_hash_value=content_hash_value,
                                    repository_revision=repository_revision,
                                    start=start,
                                    end=end,
                                )
                            )
                            break
                        symbol_kind, symbol_target = resolved_symbol
                        aliases.setdefault(star_name, symbol_target)
                        facts.append(
                            repo_local_symbol_fact(
                                subject_unit_id=module_unit_id,
                                target=symbol_target,
                                symbol_kind=symbol_kind,
                                path=path,
                                content_hash_value=content_hash_value,
                                repository_revision=repository_revision,
                                start=start,
                                end=end,
                            )
                        )
                    continue
                target = f"{relative_base}.{alias.name}" if relative_base else alias.name
                repo_resolution = repo_local_module_resolution(target, module_index)
                if repo_resolution == "resolved":
                    aliases[alias.asname or alias.name] = target
                    facts.append(
                        import_fact(
                            subject_unit_id=module_unit_id,
                            target=target,
                            path=path,
                            content_hash_value=content_hash_value,
                            repository_revision=repository_revision,
                            start=start,
                            end=end,
                            anchor_kind="repo_local_import_binding",
                        )
                    )
                elif repo_local_module_resolution(relative_base, module_index) == "resolved":
                    resolved_symbol = repo_local_symbol_resolution(relative_base, alias.name, module_symbols)
                    if resolved_symbol is not None:
                        symbol_kind, symbol_target = resolved_symbol
                        aliases[alias.asname or alias.name] = symbol_target
                        facts.append(
                            repo_local_symbol_fact(
                                subject_unit_id=module_unit_id,
                                target=symbol_target,
                                symbol_kind=symbol_kind,
                                path=path,
                                content_hash_value=content_hash_value,
                                repository_revision=repository_revision,
                                start=start,
                                end=end,
                            )
                        )
                    else:
                        facts.append(
                            unresolved_import_fact(
                                subject_unit_id=module_unit_id,
                                path=path,
                                content_hash_value=content_hash_value,
                                repository_revision=repository_revision,
                                start=start,
                                end=end,
                            )
                        )
                else:
                    facts.append(
                        unresolved_import_fact(
                            subject_unit_id=module_unit_id,
                            path=path,
                            content_hash_value=content_hash_value,
                            repository_revision=repository_revision,
                            start=start,
                            end=end,
                        )
                    )
        elif node.module:
            for alias in node.names:
                if alias.name == "*":
                    star_names = sorted((module_all_names or {}).get(node.module, set()))
                    if not star_names or repo_local_module_resolution(node.module, module_index) != "resolved":
                        facts.append(
                            unresolved_import_fact(
                                subject_unit_id=module_unit_id,
                                path=path,
                                content_hash_value=content_hash_value,
                                repository_revision=repository_revision,
                                start=start,
                                end=end,
                            )
                        )
                        continue
                    for star_name in star_names:
                        resolved_symbol = repo_local_symbol_resolution(node.module, star_name, module_symbols)
                        if resolved_symbol is None:
                            facts.append(
                                unresolved_import_fact(
                                    subject_unit_id=module_unit_id,
                                    path=path,
                                    content_hash_value=content_hash_value,
                                    repository_revision=repository_revision,
                                    start=start,
                                    end=end,
                                )
                            )
                            break
                        symbol_kind, symbol_target = resolved_symbol
                        aliases.setdefault(star_name, symbol_target)
                        facts.append(
                            repo_local_symbol_fact(
                                subject_unit_id=module_unit_id,
                                target=symbol_target,
                                symbol_kind=symbol_kind,
                                path=path,
                                content_hash_value=content_hash_value,
                                repository_revision=repository_revision,
                                start=start,
                                end=end,
                            )
                        )
                    continue
                target = f"{node.module}.{alias.name}"
                repo_resolution = repo_local_module_resolution(target, module_index)
                if repo_resolution == "ambiguous":
                    facts.append(
                        unresolved_import_fact(
                            subject_unit_id=module_unit_id,
                            path=path,
                            content_hash_value=content_hash_value,
                            repository_revision=repository_revision,
                            start=start,
                            end=end,
                        )
                    )
                    continue
                resolved_symbol = None
                if repo_resolution == "missing" and repo_local_module_resolution(node.module, module_index) == "resolved":
                    resolved_symbol = repo_local_symbol_resolution(node.module, alias.name, module_symbols)
                if repo_resolution == "missing" and resolved_symbol is None and repo_local_prefix_exists(target, module_index):
                    facts.append(
                        unresolved_import_fact(
                            subject_unit_id=module_unit_id,
                            path=path,
                            content_hash_value=content_hash_value,
                            repository_revision=repository_revision,
                            start=start,
                            end=end,
                        )
                    )
                    continue
                if resolved_symbol is not None:
                    symbol_kind, symbol_target = resolved_symbol
                    aliases[alias.asname or alias.name] = symbol_target
                    facts.append(
                        repo_local_symbol_fact(
                            subject_unit_id=module_unit_id,
                            target=symbol_target,
                            symbol_kind=symbol_kind,
                            path=path,
                            content_hash_value=content_hash_value,
                            repository_revision=repository_revision,
                            start=start,
                            end=end,
                        )
                    )
                else:
                    aliases[alias.asname or alias.name] = target
                    facts.append(
                        import_fact(
                            subject_unit_id=module_unit_id,
                            target=target,
                            path=path,
                            content_hash_value=content_hash_value,
                            repository_revision=repository_revision,
                            start=start,
                            end=end,
                            anchor_kind="repo_local_import_binding"
                            if repo_resolution == "resolved"
                            else "import_binding",
                        )
                    )
    return aliases, facts


def assignment_role(
    value: ast.AST,
    aliases: dict[str, str],
    assignments: dict[str, str],
) -> str | None:
    if isinstance(value, ast.Call):
        call_name = dotted_name(value.func)
        if not call_name:
            return None
        canonical = canonical_name(call_name, aliases, {})
        if (
            canonical
            in {
                "fastapi.APIRouter",
                "fastapi.FastAPI",
                "sqlalchemy.orm.declarative_base",
            }
            or is_constructor_like_target(canonical)
        ):
            return canonical
        return None
    if isinstance(value, ast.Name):
        return assignments.get(value.id) or aliases.get(value.id)
    name = dotted_name(value)
    if name:
        canonical = canonical_name(name, aliases, assignments)
        if canonical == "pytest.fixture":
            return canonical
    return None


def collect_assignment_roles(tree: ast.Module, aliases: dict[str, str]) -> dict[str, str]:
    assignments: dict[str, str] = {}
    for node in tree.body:
        if not isinstance(node, ast.Assign):
            continue
        role = assignment_role(node.value, aliases, assignments)
        for target in node.targets:
            if not isinstance(target, ast.Name):
                continue
            if role is None:
                assignments.pop(target.id, None)
                continue
            assignments[target.id] = role
    return assignments


def top_level_defined_names(tree: ast.Module) -> set[str]:
    return {
        node.name
        for node in tree.body
        if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef))
    }


def assignment_target_names(node: ast.AST) -> list[str]:
    if isinstance(node, ast.Assign):
        targets = list(node.targets)
    elif isinstance(node, (ast.AnnAssign, ast.AugAssign)):
        targets = [node.target]
    else:
        return []
    return [target.id for target in targets if isinstance(target, ast.Name)]


def import_local_names(node: ast.AST) -> list[str]:
    if isinstance(node, ast.Import):
        return [alias.asname or alias.name.split(".")[0] for alias in node.names]
    if isinstance(node, ast.ImportFrom):
        return [alias.asname or alias.name for alias in node.names if alias.name != "*"]
    return []


def top_level_rebound_names(node: ast.AST) -> list[str]:
    if isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef)):
        return [node.name]
    return assignment_target_names(node)


def shadowed_import_alias_names(
    tree: ast.Module,
    starts: list[int],
    aliases: dict[str, str],
) -> set[str]:
    import_offsets: dict[str, int] = {}
    rebound_offsets: dict[str, int] = {}
    for node in tree.body:
        start, _end = node_range(starts, node)
        for name in import_local_names(node):
            if name in aliases:
                import_offsets[name] = start
        for name in top_level_rebound_names(node):
            if name in aliases:
                rebound_offsets[name] = start
    return {
        name
        for name, rebound_offset in rebound_offsets.items()
        if name in import_offsets and rebound_offset > import_offsets[name]
    }


def drop_shadowed_import_aliases(
    tree: ast.Module,
    starts: list[int],
    aliases: dict[str, str],
) -> dict[str, str]:
    shadowed = shadowed_import_alias_names(tree, starts, aliases)
    if not shadowed:
        return aliases
    return {name: target for name, target in aliases.items() if name not in shadowed}


class ScopeHistoryView(Mapping[str, str]):
    """Read-only binding view backed by per-name source-ordered events."""

    def __init__(
        self,
        histories: dict[str, tuple[list[int], list[str | None]]],
        offset: int,
    ) -> None:
        self._histories = histories
        self._offset = offset

    def __getitem__(self, name: str) -> str:
        history = self._histories.get(name)
        if history is None:
            raise KeyError(name)
        positions, values = history
        index = bisect_left(positions, self._offset) - 1
        if index < 0 or values[index] is None:
            raise KeyError(name)
        return values[index]

    def __iter__(self) -> Iterator[str]:
        return (name for name in self._histories if self.get(name) is not None)

    def __len__(self) -> int:
        return sum(1 for _name in self)


class ModuleScopeTimeline:
    """Persistent module bindings without copying full state per statement."""

    def __init__(
        self,
        aliases: dict[str, str],
        alias_histories: dict[str, tuple[list[int], list[str | None]]],
        assignment_histories: dict[str, tuple[list[int], list[str | None]]],
    ) -> None:
        # Retaining the exact source mapping makes the cache identity-safe and
        # avoids re-sorting every imported name on each point query.
        self.aliases = aliases
        self.alias_histories = alias_histories
        self.assignment_histories = assignment_histories

    def aliases_at(self, offset: int) -> ScopeHistoryView:
        return ScopeHistoryView(self.alias_histories, offset)

    def assignments_at(self, offset: int) -> ScopeHistoryView:
        return ScopeHistoryView(self.assignment_histories, offset)


def record_scope_event(
    histories: dict[str, tuple[list[int], list[str | None]]],
    name: str,
    position: int,
    value: str | None,
) -> None:
    positions, values = histories.setdefault(name, ([], []))
    if values and values[-1] == value:
        return
    positions.append(position)
    values.append(value)


def aliases_at_offset(
    tree: ast.Module,
    starts: list[int],
    aliases: dict[str, str],
    offset: int,
) -> Mapping[str, str]:
    return module_scope_timeline(tree, starts, aliases).aliases_at(offset)


def module_scope_timeline(
    tree: ast.Module,
    starts: list[int],
    aliases: dict[str, str],
) -> ModuleScopeTimeline:
    cached = getattr(tree, "_repogrammar_scope_timeline", None)
    if cached is not None and cached.aliases is aliases:
        return cached

    alias_histories: dict[str, tuple[list[int], list[str | None]]] = {}
    assignment_histories: dict[str, tuple[list[int], list[str | None]]] = {}
    visible: dict[str, str] = {}
    assignments: dict[str, str] = {}
    for node in sorted(tree.body, key=lambda item: node_range(starts, item)):
        start, _end = node_range(starts, node)
        if isinstance(node, ast.Assign):
            role = assignment_role(node.value, visible, assignments)
            for target in node.targets:
                if not isinstance(target, ast.Name):
                    continue
                if role is None:
                    assignments.pop(target.id, None)
                else:
                    assignments[target.id] = role
                record_scope_event(assignment_histories, target.id, start, role)
        for name in import_local_names(node):
            if name in aliases:
                visible[name] = aliases[name]
                record_scope_event(alias_histories, name, start, aliases[name])
        for name in top_level_rebound_names(node):
            if name in visible:
                visible.pop(name)
                record_scope_event(alias_histories, name, start, None)

    result = ModuleScopeTimeline(aliases, alias_histories, assignment_histories)
    setattr(tree, "_repogrammar_scope_timeline", result)
    return result


def collect_assignment_roles_until(
    tree: ast.Module,
    starts: list[int],
    aliases: dict[str, str],
    offset: int,
) -> Mapping[str, str]:
    return module_scope_timeline(tree, starts, aliases).assignments_at(offset)


def collect_parameter_roles(
    node: ast.FunctionDef | ast.AsyncFunctionDef,
    aliases: dict[str, str],
) -> dict[str, str]:
    roles: dict[str, str] = {}
    args = [
        *getattr(node.args, "posonlyargs", []),
        *node.args.args,
        *node.args.kwonlyargs,
    ]
    for arg in args:
        annotation = dotted_name(arg.annotation) if arg.annotation else None
        if not annotation:
            continue
        canonical = canonical_name(annotation, aliases, {})
        if canonical in SQLALCHEMY_SESSION_TYPES:
            roles[arg.arg] = canonical
    return roles


def assigned_role_receivers(node: ast.AST) -> set[str]:
    receivers: set[str] = set()
    for child in ast.walk(node):
        targets: list[ast.AST] = []
        if isinstance(child, ast.Assign):
            targets = list(child.targets)
        elif isinstance(child, (ast.AnnAssign, ast.AugAssign)):
            targets = [child.target]
        for target in targets:
            if name := dotted_name(target):
                receivers.add(name)
    return receivers


def collect_instance_attribute_roles(
    node: ast.ClassDef,
    aliases: dict[str, str],
) -> dict[str, str]:
    roles: dict[str, str] = {}
    for child in node.body:
        if (
            not isinstance(child, (ast.FunctionDef, ast.AsyncFunctionDef))
            or child.name != "__init__"
        ):
            continue
        parameter_roles = collect_parameter_roles(child, aliases)
        for statement in child.body:
            targets: list[ast.AST] = []
            if isinstance(statement, ast.Assign):
                targets = list(statement.targets)
            elif isinstance(statement, ast.AnnAssign):
                targets = [statement.target]
            else:
                continue
            role = None
            value = getattr(statement, "value", None)
            if isinstance(value, ast.Name):
                role = parameter_roles.get(value.id)
            if (
                role is None
                and isinstance(statement, ast.AnnAssign)
                and isinstance(value, ast.Name)
            ):
                annotation = dotted_name(statement.annotation)
                canonical = canonical_name(annotation, aliases, {}) if annotation else None
                if canonical in SQLALCHEMY_SESSION_TYPES:
                    role = canonical
            for target in targets:
                target_name = dotted_name(target)
                if not target_name or not target_name.startswith("self."):
                    continue
                if role is None:
                    roles.pop(target_name, None)
                else:
                    roles[target_name] = role
    return roles


def function_has_typed_sqlalchemy_query_call(
    node: ast.FunctionDef | ast.AsyncFunctionDef,
    aliases: dict[str, str],
    parameter_roles: dict[str, str] | None = None,
) -> bool:
    roles = parameter_roles or collect_parameter_roles(node, aliases)
    shadowed_receivers = assigned_role_receivers(node)
    for child in ast.walk(node):
        if not isinstance(child, ast.Call):
            continue
        name = dotted_name(child.func)
        if not name:
            continue
        canonical = canonical_name(name, aliases, {})
        parts = canonical.split(".")
        if len(parts) < 2:
            continue
        receiver = ".".join(parts[:-1])
        if (
            receiver in roles
            and receiver not in shadowed_receivers
            and parts[-1] in SQLALCHEMY_CUSTOM_QUERY_WRAPPER_METHODS
        ):
            return True
    return False


def collect_sqlalchemy_custom_query_wrapper_names(
    node: ast.Module | ast.ClassDef,
    aliases: dict[str, str],
    instance_attribute_roles: dict[str, str] | None = None,
) -> set[str]:
    names: set[str] = set()
    for child in node.body:
        if not isinstance(child, (ast.FunctionDef, ast.AsyncFunctionDef)):
            continue
        parameter_roles = collect_parameter_roles(child, aliases)
        if instance_attribute_roles:
            parameter_roles = {**parameter_roles, **instance_attribute_roles}
        if function_has_typed_sqlalchemy_query_call(child, aliases, parameter_roles):
            names.add(child.name)
    return names


def is_sqlalchemy_custom_query_wrapper_call(
    canonical: str | None,
    custom_query_wrappers: set[str],
) -> bool:
    if not canonical:
        return False
    if canonical in custom_query_wrappers:
        return True
    return (
        canonical.startswith("self.")
        and canonical.removeprefix("self.") in custom_query_wrappers
    )


def add_fact(facts: list[dict[str, Any]], new_fact: dict[str, Any]) -> None:
    target = new_fact.get("target")
    if target is not None and not is_safe_fact_target(target):
        return
    if len(facts) < MAX_FACTS_PER_FILE:
        facts.append(new_fact)


def collect_module_identity_and_scope_facts(
    tree: ast.Module,
    path: str,
    source: str,
    content_hash_value: str,
    repository_revision: str,
    module_unit_id: str,
    facts: list[dict[str, Any]],
) -> None:
    end = len(source.encode("utf-8"))
    module_name = module_name_from_path(path)
    if module_name and is_safe_fact_target(module_name):
        add_fact(
            facts,
            structural_fact(
                kind="SYMBOL",
                subject_unit_id=module_unit_id,
                target=module_name,
                path=path,
                content_hash_value=content_hash_value,
                repository_revision=repository_revision,
                start=0,
                end=end,
                anchor_kind="module_name",
            ),
        )
    try:
        table = symtable.symtable(source, path, "exec")
    except (SyntaxError, ValueError, TypeError):
        return
    for name in sorted(table.get_identifiers()):
        if not is_python_identifier(name):
            continue
        symbol = table.lookup(name)
        if symbol.is_imported():
            scope_kind = "scope_imported"
            target = f"scope.imported.{name}"
        elif symbol.is_namespace():
            scope_kind = "scope_namespace"
            target = f"scope.namespace.{name}"
        elif symbol.is_assigned():
            scope_kind = "scope_assigned"
            target = f"scope.assigned.{name}"
        else:
            continue
        add_fact(
            facts,
            structural_fact(
                kind="SYMBOL",
                subject_unit_id=module_unit_id,
                target=target,
                path=path,
                content_hash_value=content_hash_value,
                repository_revision=repository_revision,
                start=0,
                end=end,
                anchor_kind=scope_kind,
            ),
        )


def collect_decorator_facts(
    node: ast.AST,
    starts: list[int],
    path: str,
    content_hash_value: str,
    repository_revision: str,
    subject_unit_id: str,
    aliases: dict[str, str],
    assignments: dict[str, str],
    defined_names: set[str],
    facts: list[dict[str, Any]],
) -> None:
    for decorator in getattr(node, "decorator_list", []):
        start, end = node_range(starts, decorator)
        name = dotted_name(decorator)
        if not name:
            add_fact(
                facts,
                unknown_fact(
                    subject_unit_id=subject_unit_id,
                    reason_code="FrameworkMagic",
                    affected_claim="python_framework_identity",
                    path=path,
                    content_hash_value=content_hash_value,
                    repository_revision=repository_revision,
                    start=start,
                    end=end,
                ),
            )
            continue
        target = canonical_name(name, aliases, assignments)
        anchor_kind = decorator_anchor_kind(target)
        add_fact(
            facts,
            structural_fact(
                kind="SYMBOL",
                subject_unit_id=subject_unit_id,
                target=target,
                path=path,
                content_hash_value=content_hash_value,
                repository_revision=repository_revision,
                start=start,
                end=end,
                anchor_kind=anchor_kind,
            ),
        )
        if decorator_binding_needs_unknown(
            decorator, name, target, anchor_kind, aliases, assignments, defined_names
        ):
            add_fact(
                facts,
                unknown_fact(
                    subject_unit_id=subject_unit_id,
                    reason_code="FrameworkMagic",
                    affected_claim="python_framework_identity",
                    path=path,
                    content_hash_value=content_hash_value,
                    repository_revision=repository_revision,
                    start=start,
                    end=end,
                ),
            )
        if anchor_kind == "fastapi_route_decorator" and isinstance(decorator, ast.Call):
            collect_fastapi_response_model_facts(
                decorator,
                starts,
                path,
                content_hash_value,
                repository_revision,
                subject_unit_id,
                aliases,
                facts,
            )


def decorator_binding_needs_unknown(
    decorator: ast.AST,
    name: str,
    target: str,
    anchor_kind: str,
    aliases: dict[str, str],
    assignments: dict[str, str],
    defined_names: set[str],
) -> bool:
    if anchor_kind != "decorator_binding":
        return False
    if isinstance(decorator, ast.Call):
        return True
    root = name.split(".", 1)[0]
    if target in SAFE_NATIVE_DECORATORS:
        return False
    if root in defined_names or root in aliases or root in assignments:
        return False
    return True


def has_pydantic_runtime_validator_decorator(
    node: ast.AST,
    aliases: dict[str, str],
    assignments: dict[str, str],
) -> bool:
    if not isinstance(node, (ast.FunctionDef, ast.AsyncFunctionDef)):
        return False
    for decorator in node.decorator_list:
        name = dotted_name(decorator)
        if not name:
            continue
        if canonical_name(name, aliases, assignments) in PYDANTIC_RUNTIME_VALIDATOR_TARGETS:
            return True
    return False


def function_body_start_byte(
    node: ast.FunctionDef | ast.AsyncFunctionDef,
    starts: list[int],
) -> int | None:
    if not node.body:
        return None
    start, _end = node_range(starts, node.body[0])
    return start


def collect_fastapi_response_model_facts(
    decorator: ast.Call,
    starts: list[int],
    path: str,
    content_hash_value: str,
    repository_revision: str,
    subject_unit_id: str,
    aliases: dict[str, str],
    facts: list[dict[str, Any]],
) -> None:
    for keyword in decorator.keywords:
        if keyword.arg != "response_model":
            continue
        name = static_type_name(keyword.value)
        if not name:
            continue
        model_name = canonical_name(name, aliases, {})
        target = f"fastapi.response_model.{model_name}"
        if not is_safe_fact_target(target) or len(target) > MAX_RUST_PARSE_FACT_TARGET_CHARS:
            continue
        start, end = node_range(starts, keyword.value)
        add_fact(
            facts,
            structural_fact(
                kind="TYPE",
                subject_unit_id=subject_unit_id,
                target=target,
                path=path,
                content_hash_value=content_hash_value,
                repository_revision=repository_revision,
                start=start,
                end=end,
                anchor_kind="fastapi_response_model",
            ),
        )


def literal_string_value(node: ast.AST | None) -> str | None:
    if isinstance(node, ast.Constant) and isinstance(node.value, str):
        return node.value
    return None


def normalize_route_prefix(value: str) -> str:
    stripped = value.strip("/")
    if not stripped:
        return "/"
    segments = []
    for segment in stripped.split("/"):
        if not segment:
            continue
        if segment.startswith("{") and segment.endswith("}"):
            segments.append(":param")
        elif any(character in segment for character in "*?"):
            segments.append(":pattern")
        elif segment.isdigit():
            segments.append(":number")
        else:
            # Preserve only the route shape. A literal prefix is source text
            # and must not cross the parser boundary through assumptions.
            segments.append(":literal")
    return "/" + "/".join(segments) if segments else "/"


def fastapi_include_router_prefix_shape(call: ast.Call) -> tuple[str | None, bool]:
    for keyword in call.keywords:
        if keyword.arg == "prefix":
            value = literal_string_value(keyword.value)
            if value is None:
                return None, True
            return normalize_route_prefix(value), False
    return "none", False


def fastapi_include_router_binding_assumptions(
    router_name: str | None,
    aliases: dict[str, str],
    assignments: dict[str, str],
    module_index: dict[str, list[str]] | None,
) -> list[str] | None:
    if not router_name:
        return None
    root_name = router_name.split(".", 1)[0]
    canonical = canonical_name(router_name, aliases, assignments)
    if assignments.get(root_name) == "fastapi.APIRouter" and router_name == root_name:
        return ["router_binding=local", f"router_local_name={root_name}"]
    if root_name in aliases:
        module_name = canonical.rsplit(".", 1)[0]
        if module_index is not None and (
            module_name in module_index or repo_local_prefix_exists(module_name, module_index)
        ):
            return ["router_binding=repo_local_import", f"router_target={canonical}"]
        return None
    return None


def collect_fastapi_include_router_facts(
    tree: ast.Module,
    starts: list[int],
    path: str,
    content_hash_value: str,
    repository_revision: str,
    module_unit_id: str,
    aliases: dict[str, str],
    assignments: dict[str, str],
    module_index: dict[str, list[str]] | None,
    facts: list[dict[str, Any]],
) -> None:
    for item in tree.body:
        if not isinstance(item, ast.Expr) or not isinstance(item.value, ast.Call):
            continue
        call = item.value
        start, end = node_range(starts, call)
        name = dotted_name(call.func)
        canonical = canonical_name(name, aliases, assignments) if name else None
        if canonical not in {"fastapi.FastAPI.include_router", "fastapi.APIRouter.include_router"}:
            continue
        router_name = static_reference_name(call.args[0]) if call.args else None
        binding_assumptions = fastapi_include_router_binding_assumptions(
            router_name,
            aliases,
            assignments,
            module_index,
        )
        if binding_assumptions is None:
            add_fact(
                facts,
                unknown_fact(
                    subject_unit_id=module_unit_id,
                    reason_code="UnresolvedImport",
                    affected_claim="fastapi_router_binding",
                    path=path,
                    content_hash_value=content_hash_value,
                    repository_revision=repository_revision,
                    start=start,
                    end=end,
                ),
            )
            continue
        prefix_shape, prefix_unknown = fastapi_include_router_prefix_shape(call)
        if prefix_unknown:
            add_fact(
                facts,
                unknown_fact(
                    subject_unit_id=module_unit_id,
                    reason_code="FrameworkMagic",
                    affected_claim="fastapi_router_prefix",
                    path=path,
                    content_hash_value=content_hash_value,
                    repository_revision=repository_revision,
                    start=start,
                    end=end,
                ),
            )
            continue
        fact_data = structural_fact(
            kind="RESOLVED_CALL",
            subject_unit_id=module_unit_id,
            target=canonical,
            path=path,
            content_hash_value=content_hash_value,
            repository_revision=repository_revision,
            start=start,
            end=end,
            anchor_kind="fastapi_include_router",
        )
        fact_data["assumptions"].extend(
            [
                "fact_scope=context_only",
                "prefix_unknown=false",
                f"route_prefix_shape={prefix_shape}",
                *binding_assumptions,
            ]
        )
        add_fact(facts, fact_data)


def annotated_type_and_metadata(
    annotation: ast.AST | None,
    aliases: dict[str, str],
) -> tuple[ast.AST | None, list[ast.AST]]:
    if not isinstance(annotation, ast.Subscript):
        return annotation, []
    name = dotted_name(annotation.value)
    canonical = canonical_name(name, aliases, {}) if name else None
    if canonical not in {"typing.Annotated", "Annotated"}:
        return annotation, []
    if isinstance(annotation.slice, ast.Tuple):
        elements = annotation.slice.elts
    else:
        elements = [annotation.slice]
    if not elements:
        return None, []
    return elements[0], elements[1:]


def fastapi_parameter_marker(
    node: ast.AST | None,
    aliases: dict[str, str],
) -> tuple[str, str] | None:
    if not isinstance(node, ast.Call):
        return None
    name = dotted_name(node.func)
    if not name:
        return None
    return FASTAPI_PARAMETER_MARKERS.get(canonical_name(name, aliases, {}))


def fastapi_parameter_annotation_target(
    annotation: ast.AST | None,
    aliases: dict[str, str],
) -> str | None:
    type_node, _ = annotated_type_and_metadata(annotation, aliases)
    if type_node is None:
        return None
    name = static_type_name(type_node)
    return canonical_name(name, aliases, {}) if name else None


def function_parameters(
    node: ast.FunctionDef | ast.AsyncFunctionDef,
) -> list[tuple[ast.arg, ast.AST | None]]:
    positional_args = [*node.args.posonlyargs, *node.args.args]
    padding = [None] * (len(positional_args) - len(node.args.defaults))
    parameters = list(zip(positional_args, [*padding, *node.args.defaults]))
    parameters.extend(zip(node.args.kwonlyargs, node.args.kw_defaults))
    return parameters


def collect_fastapi_parameter_facts(
    node: ast.FunctionDef | ast.AsyncFunctionDef,
    starts: list[int],
    path: str,
    content_hash_value: str,
    repository_revision: str,
    subject_unit_id: str,
    aliases: dict[str, str],
    facts: list[dict[str, Any]],
) -> None:
    for parameter, default in function_parameters(node):
        if parameter.arg in {"self", "cls"}:
            continue
        type_node, metadata = annotated_type_and_metadata(parameter.annotation, aliases)
        marker = fastapi_parameter_marker(default, aliases)
        if marker is None:
            marker = next(
                (
                    candidate
                    for item in metadata
                    if (candidate := fastapi_parameter_marker(item, aliases)) is not None
                ),
                None,
            )
        if marker is None:
            continue
        anchor_kind, prefix = marker
        if anchor_kind == "fastapi_request_body_model":
            type_name = fastapi_parameter_annotation_target(type_node, aliases)
            if not type_name:
                continue
            target = f"{prefix}.{type_name}"
            fact_kind = "TYPE"
            start, end = node_range(starts, type_node)
        else:
            if not is_python_identifier(parameter.arg):
                continue
            target = f"{prefix}.{parameter.arg}"
            fact_kind = "SYMBOL"
            start, end = node_range(starts, parameter)
        if not is_safe_fact_target(target) or len(target) > MAX_RUST_PARSE_FACT_TARGET_CHARS:
            continue
        add_fact(
            facts,
            structural_fact(
                kind=fact_kind,
                subject_unit_id=subject_unit_id,
                target=target,
                path=path,
                content_hash_value=content_hash_value,
                repository_revision=repository_revision,
                start=start,
                end=end,
                anchor_kind=anchor_kind,
            ),
        )


def decorator_anchor_kind(target: str) -> str:
    parts = target.split(".")
    if target == "pytest.fixture":
        return "pytest_fixture_decorator"
    if target == "pytest.mark.parametrize":
        return "pytest_parametrize"
    if target == "pydantic.computed_field":
        return "pydantic_computed_field"
    if target == "pydantic.model_validator":
        return "pydantic_model_validator"
    if target in PYDANTIC_VALIDATOR_TARGETS:
        return "pydantic_validator"
    if (
        len(parts) >= 3
        and ".".join(parts[:-1]) in {"fastapi.FastAPI", "fastapi.APIRouter"}
        and parts[-1] in ROUTE_METHODS
    ):
        return "fastapi_route_decorator"
    if (
        len(parts) >= 3
        and ".".join(parts[:-1]) in FLASK_APP_RECEIVER_TYPES
        and parts[-1] in FLASK_ROUTE_ATTRS
    ):
        return "flask_route_decorator"
    if target in CLICK_COMMAND_DECORATORS or target in CLICK_PARAM_DECORATORS:
        return "click_command_decorator"
    if (
        len(parts) >= 3
        and ".".join(parts[:-1]) == TYPER_APP_RECEIVER_TYPE
        and parts[-1] == "command"
    ):
        return "typer_command_decorator"
    if target == CELERY_SHARED_TASK or (
        len(parts) >= 3
        and ".".join(parts[:-1]) == CELERY_APP_RECEIVER_TYPE
        and parts[-1] == "task"
    ):
        return "celery_task_decorator"
    return "decorator_binding"


def pytest_parametrize_names(
    node: ast.FunctionDef | ast.AsyncFunctionDef,
    aliases: dict[str, str],
    assignments: dict[str, str],
) -> set[str]:
    return pytest_parametrize_name_sets(node, aliases, assignments)[0]


def pytest_parametrize_name_sets(
    node: ast.FunctionDef | ast.AsyncFunctionDef,
    aliases: dict[str, str],
    assignments: dict[str, str],
) -> tuple[set[str], set[str]]:
    direct_names: set[str] = set()
    indirect_names: set[str] = set()
    for decorator in getattr(node, "decorator_list", []):
        if not isinstance(decorator, ast.Call):
            continue
        name = dotted_name(decorator.func)
        if not name or canonical_name(name, aliases, assignments) != "pytest.mark.parametrize":
            continue
        first_arg = decorator.args[0] if decorator.args else None
        decorator_names = pytest_literal_name_set(first_arg)
        if not decorator_names:
            continue
        decorator_indirect_names = pytest_parametrize_indirect_names(decorator, decorator_names)
        direct_names.update(decorator_names - decorator_indirect_names)
        indirect_names.update(decorator_indirect_names)
    return direct_names, indirect_names


def pytest_literal_name_set(value: ast.AST | None) -> set[str]:
    names: set[str] = set()
    if isinstance(value, ast.Constant) and isinstance(value.value, str):
        names.update(
            item.strip()
            for item in value.value.split(",")
            if is_python_identifier(item.strip())
        )
    elif isinstance(value, (ast.Tuple, ast.List)):
        for item in value.elts:
            if (
                isinstance(item, ast.Constant)
                and isinstance(item.value, str)
                and is_python_identifier(item.value)
            ):
                names.add(item.value)
    return names


def pytest_parametrize_indirect_names(decorator: ast.Call, names: set[str]) -> set[str]:
    for keyword in decorator.keywords:
        if keyword.arg != "indirect":
            continue
        value = keyword.value
        if isinstance(value, ast.Constant):
            if value.value is True:
                return set(names)
            if value.value is False or value.value is None:
                return set()
            if isinstance(value.value, str):
                return pytest_literal_name_set(value)
        if isinstance(value, (ast.Tuple, ast.List)):
            indirect_names = pytest_literal_name_set(value)
            if all(
                isinstance(item, ast.Constant)
                and isinstance(item.value, str)
                and is_python_identifier(item.value)
                for item in value.elts
            ):
                return indirect_names
        return set(names)
    return set()


def collect_class_base_facts(
    node: ast.ClassDef,
    starts: list[int],
    path: str,
    content_hash_value: str,
    repository_revision: str,
    subject_unit_id: str,
    aliases: dict[str, str],
    assignments: dict[str, str],
    facts: list[dict[str, Any]],
) -> None:
    for base in node.bases:
        name = dotted_name(base)
        if not name:
            continue
        start, end = node_range(starts, base)
        add_fact(
            facts,
            structural_fact(
                kind="TYPE",
                subject_unit_id=subject_unit_id,
                target=canonical_name(name, aliases, assignments),
                path=path,
                content_hash_value=content_hash_value,
                repository_revision=repository_revision,
                start=start,
                end=end,
                anchor_kind="class_base",
            ),
        )


def collect_external_framework_base_unknown_facts(
    node: ast.ClassDef,
    starts: list[int],
    path: str,
    content_hash_value: str,
    repository_revision: str,
    subject_unit_id: str,
    aliases: dict[str, str],
    assignments: dict[str, str],
    facts: list[dict[str, Any]],
) -> None:
    external_bases: list[tuple[ast.AST, str]] = []
    if class_has_pydantic_member_signal(node, aliases):
        external_bases.extend(
            (base, "python_framework_identity")
            for base in external_framework_base_nodes(
                node,
                aliases,
                assignments,
                ("pydantic.", "pydantic_settings."),
                PYDANTIC_MODEL_BASES,
            )
        )
    if class_has_sqlalchemy_member_signal(node, aliases, assignments):
        external_bases.extend(
            (base, "python_framework_identity")
            for base in external_framework_base_nodes(
                node,
                aliases,
                assignments,
                ("sqlalchemy.",),
                SQLALCHEMY_MODEL_BASES,
            )
        )
    if class_has_django_model_lookalike_base(node, aliases, assignments):
        external_bases.extend(
            (base, "python_django_model_identity")
            for base in external_framework_base_nodes(
                node,
                aliases,
                assignments,
                ("django.",),
                DJANGO_MODEL_BASES,
            )
        )
    seen_ranges: set[tuple[int, int]] = set()
    for base, affected_claim in external_bases:
        start, end = node_range(starts, base)
        if (start, end) in seen_ranges:
            continue
        seen_ranges.add((start, end))
        add_fact(
            facts,
            unknown_fact(
                subject_unit_id=subject_unit_id,
                reason_code="FrameworkMagic",
                affected_claim=affected_claim,
                path=path,
                content_hash_value=content_hash_value,
                repository_revision=repository_revision,
                start=start,
                end=end,
            ),
        )


def django_field_count(
    node: ast.ClassDef,
    aliases: dict[str, str],
    assignments: dict[str, str],
) -> int:
    count = 0
    for item in node.body:
        if not isinstance(item, ast.Assign) or not isinstance(item.value, ast.Call):
            continue
        name = dotted_name(item.value.func)
        if not name:
            continue
        if canonical_name(name, aliases, assignments).startswith(DJANGO_FIELD_MODULE_PREFIX):
            count += 1
    return count


def django_has_meta(node: ast.ClassDef) -> bool:
    return any(isinstance(item, ast.ClassDef) and item.name == "Meta" for item in node.body)


def django_test_method_count(node: ast.ClassDef) -> int:
    return sum(
        1
        for item in node.body
        if isinstance(item, (ast.FunctionDef, ast.AsyncFunctionDef))
        and item.name.startswith("test_")
    )


def unittest_fixture_shape(node: ast.ClassDef) -> str:
    names = {
        item.name
        for item in node.body
        if isinstance(item, (ast.FunctionDef, ast.AsyncFunctionDef))
    }
    has_setup = "setUp" in names
    has_teardown = "tearDown" in names
    if has_setup and has_teardown:
        return "setup_teardown"
    if has_setup:
        return "setup_only"
    if has_teardown:
        return "teardown_only"
    return "none"


def flask_methods_literal(value: ast.AST) -> list[str] | None:
    if not isinstance(value, (ast.List, ast.Tuple)):
        return None
    methods: list[str] = []
    for element in value.elts:
        if isinstance(element, ast.Constant) and isinstance(element.value, str):
            methods.append(element.value.lower())
        else:
            return None
    return methods


def flask_route_http_method(decorator: ast.AST, attr: str) -> str:
    if attr in FLASK_METHOD_ATTRS:
        return attr
    if isinstance(decorator, ast.Call):
        for keyword in decorator.keywords:
            if keyword.arg != "methods":
                continue
            methods = flask_methods_literal(keyword.value)
            if methods and len(methods) == 1 and is_python_identifier(methods[0]):
                return methods[0]
    return "request"


def cli_param_count(
    node: ast.FunctionDef | ast.AsyncFunctionDef,
    unit_kind: str,
    aliases: dict[str, str],
    assignments: dict[str, str],
) -> int:
    if unit_kind == "click_command":
        return sum(
            1
            for name in canonical_decorator_names(node, aliases, assignments)
            if name in CLICK_PARAM_DECORATORS
        )
    parameters = [*node.args.posonlyargs, *node.args.args, *node.args.kwonlyargs]
    return sum(1 for parameter in parameters if parameter.arg not in {"self", "cls"})


def collect_django_model_facts(
    node: ast.ClassDef,
    starts: list[int],
    path: str,
    content_hash_value: str,
    repository_revision: str,
    subject_unit_id: str,
    aliases: dict[str, str],
    assignments: dict[str, str],
    facts: list[dict[str, Any]],
) -> None:
    start, end = node_range(starts, node)
    field_count = django_field_count(node, aliases, assignments)
    if field_count > 0:
        add_fact(
            facts,
            structural_fact(
                kind="SYMBOL",
                subject_unit_id=subject_unit_id,
                target=f"django.field_count.{count_shape_bucket(field_count)}",
                path=path,
                content_hash_value=content_hash_value,
                repository_revision=repository_revision,
                start=start,
                end=end,
                anchor_kind="django_model_field",
            ),
        )
    if django_has_meta(node):
        add_fact(
            facts,
            structural_fact(
                kind="SYMBOL",
                subject_unit_id=subject_unit_id,
                target="django.model_meta.present",
                path=path,
                content_hash_value=content_hash_value,
                repository_revision=repository_revision,
                start=start,
                end=end,
                anchor_kind="django_model_meta",
            ),
        )
    add_fact(
        facts,
        unknown_fact(
            subject_unit_id=subject_unit_id,
            reason_code="FrameworkMagic",
            affected_claim="python_django_settings_behavior",
            path=path,
            content_hash_value=content_hash_value,
            repository_revision=repository_revision,
            start=start,
            end=end,
        ),
    )


def collect_django_test_facts(
    node: ast.ClassDef,
    starts: list[int],
    path: str,
    content_hash_value: str,
    repository_revision: str,
    subject_unit_id: str,
    facts: list[dict[str, Any]],
) -> None:
    start, end = node_range(starts, node)
    method_count = django_test_method_count(node)
    if method_count > 0:
        add_fact(
            facts,
            structural_fact(
                kind="SYMBOL",
                subject_unit_id=subject_unit_id,
                target=f"django.test_method_count.{count_shape_bucket(method_count)}",
                path=path,
                content_hash_value=content_hash_value,
                repository_revision=repository_revision,
                start=start,
                end=end,
                anchor_kind="django_test_method",
            ),
        )
    add_fact(
        facts,
        unknown_fact(
            subject_unit_id=subject_unit_id,
            reason_code="FrameworkMagic",
            affected_claim="python_django_settings_behavior",
            path=path,
            content_hash_value=content_hash_value,
            repository_revision=repository_revision,
            start=start,
            end=end,
        ),
    )


def collect_flask_route_facts(
    node: ast.FunctionDef | ast.AsyncFunctionDef,
    starts: list[int],
    path: str,
    content_hash_value: str,
    repository_revision: str,
    subject_unit_id: str,
    aliases: dict[str, str],
    assignments: dict[str, str],
    facts: list[dict[str, Any]],
) -> None:
    for decorator in getattr(node, "decorator_list", []):
        func = decorator.func if isinstance(decorator, ast.Call) else decorator
        raw = dotted_name(func)
        if not raw:
            continue
        canonical = canonical_name(raw, aliases, assignments)
        parts = canonical.split(".")
        if (
            len(parts) < 3
            or ".".join(parts[:-1]) not in FLASK_APP_RECEIVER_TYPES
            or parts[-1] not in FLASK_ROUTE_ATTRS
        ):
            continue
        start, end = node_range(starts, decorator)
        path_literal = (
            literal_string_value(decorator.args[0])
            if isinstance(decorator, ast.Call) and decorator.args
            else None
        )
        if path_literal is None:
            add_fact(
                facts,
                unknown_fact(
                    subject_unit_id=subject_unit_id,
                    reason_code="FrameworkMagic",
                    affected_claim="python_flask_route_identity",
                    path=path,
                    content_hash_value=content_hash_value,
                    repository_revision=repository_revision,
                    start=start,
                    end=end,
                ),
            )
            return
        add_fact(
            facts,
            structural_fact(
                kind="SYMBOL",
                subject_unit_id=subject_unit_id,
                target="flask.route",
                path=path,
                content_hash_value=content_hash_value,
                repository_revision=repository_revision,
                start=start,
                end=end,
                anchor_kind="flask_route_decorator",
            ),
        )
        method = flask_route_http_method(decorator, parts[-1])
        add_fact(
            facts,
            structural_fact(
                kind="SYMBOL",
                subject_unit_id=subject_unit_id,
                target=f"flask.http_method.{method}",
                path=path,
                content_hash_value=content_hash_value,
                repository_revision=repository_revision,
                start=start,
                end=end,
                anchor_kind="flask_route_method",
            ),
        )
        return


def collect_cli_command_facts(
    node: ast.FunctionDef | ast.AsyncFunctionDef,
    unit_kind: str,
    starts: list[int],
    path: str,
    content_hash_value: str,
    repository_revision: str,
    subject_unit_id: str,
    aliases: dict[str, str],
    assignments: dict[str, str],
    facts: list[dict[str, Any]],
) -> None:
    start, end = node_range(starts, node)
    target = "click.command" if unit_kind == "click_command" else "typer.command"
    anchor = (
        "click_command_decorator"
        if unit_kind == "click_command"
        else "typer_command_decorator"
    )
    add_fact(
        facts,
        structural_fact(
            kind="SYMBOL",
            subject_unit_id=subject_unit_id,
            target=target,
            path=path,
            content_hash_value=content_hash_value,
            repository_revision=repository_revision,
            start=start,
            end=end,
            anchor_kind=anchor,
        ),
    )
    param_count = cli_param_count(node, unit_kind, aliases, assignments)
    add_fact(
        facts,
        structural_fact(
            kind="SYMBOL",
            subject_unit_id=subject_unit_id,
            target=f"cli.param_count.{count_shape_bucket(param_count)}",
            path=path,
            content_hash_value=content_hash_value,
            repository_revision=repository_revision,
            start=start,
            end=end,
            anchor_kind="cli_param_count",
        ),
    )


def collect_celery_task_facts(
    node: ast.FunctionDef | ast.AsyncFunctionDef,
    starts: list[int],
    path: str,
    content_hash_value: str,
    repository_revision: str,
    subject_unit_id: str,
    aliases: dict[str, str],
    assignments: dict[str, str],
    facts: list[dict[str, Any]],
) -> None:
    target: str | None = None
    for decorator in getattr(node, "decorator_list", []):
        func = decorator.func if isinstance(decorator, ast.Call) else decorator
        raw = dotted_name(func)
        if not raw:
            continue
        canonical = canonical_name(raw, aliases, assignments)
        parts = canonical.split(".")
        if canonical == CELERY_SHARED_TASK:
            target = "celery.shared_task"
            break
        if (
            len(parts) >= 3
            and ".".join(parts[:-1]) == CELERY_APP_RECEIVER_TYPE
            and parts[-1] == "task"
        ):
            target = "celery.task"
            break
    if target is None:
        return
    start, end = node_range(starts, node)
    add_fact(
        facts,
        structural_fact(
            kind="SYMBOL",
            subject_unit_id=subject_unit_id,
            target=target,
            path=path,
            content_hash_value=content_hash_value,
            repository_revision=repository_revision,
            start=start,
            end=end,
            anchor_kind="celery_task_decorator",
        ),
    )
    for child in ast.walk(node):
        if not isinstance(child, ast.Call):
            continue
        attr = child.func.attr if isinstance(child.func, ast.Attribute) else None
        if attr in CELERY_RUNTIME_METHODS or attr == CELERY_SEND_TASK_ATTR:
            call_start, call_end = node_range(starts, child)
            add_fact(
                facts,
                unknown_fact(
                    subject_unit_id=subject_unit_id,
                    reason_code="FrameworkMagic",
                    affected_claim="python_celery_runtime_routing",
                    path=path,
                    content_hash_value=content_hash_value,
                    repository_revision=repository_revision,
                    start=call_start,
                    end=call_end,
                ),
            )
            return


def collect_unittest_test_facts(
    method_node: ast.FunctionDef | ast.AsyncFunctionDef,
    class_node: ast.ClassDef,
    starts: list[int],
    path: str,
    content_hash_value: str,
    repository_revision: str,
    subject_unit_id: str,
    aliases: dict[str, str],
    assignments: dict[str, str],
    facts: list[dict[str, Any]],
) -> None:
    start, end = node_range(starts, method_node)
    add_fact(
        facts,
        structural_fact(
            kind="SYMBOL",
            subject_unit_id=subject_unit_id,
            target="unittest.TestCase.test",
            path=path,
            content_hash_value=content_hash_value,
            repository_revision=repository_revision,
            start=start,
            end=end,
            anchor_kind="unittest_test_method",
        ),
    )
    add_fact(
        facts,
        structural_fact(
            kind="SYMBOL",
            subject_unit_id=subject_unit_id,
            target=f"unittest.fixture.{unittest_fixture_shape(class_node)}",
            path=path,
            content_hash_value=content_hash_value,
            repository_revision=repository_revision,
            start=start,
            end=end,
            anchor_kind="unittest_fixture",
        ),
    )
    for decorator in getattr(method_node, "decorator_list", []):
        func = decorator.func if isinstance(decorator, ast.Call) else decorator
        raw = dotted_name(func)
        if not raw:
            continue
        if canonical_name(raw, aliases, assignments) == UNITTEST_PATCH_TARGET:
            patch_start, patch_end = node_range(starts, decorator)
            add_fact(
                facts,
                unknown_fact(
                    subject_unit_id=subject_unit_id,
                    reason_code="MonkeyPatch",
                    affected_claim="python_unittest_patch_target",
                    path=path,
                    content_hash_value=content_hash_value,
                    repository_revision=repository_revision,
                    start=patch_start,
                    end=patch_end,
                ),
            )


def collect_django_url_patterns(
    tree: ast.Module,
    starts: list[int],
    path: str,
    content_hash_value: str,
    repository_revision: str,
    aliases: dict[str, str],
    module_unit_id: str,
    units: list[dict[str, Any]],
    facts: list[dict[str, Any]],
    ordinal: int,
) -> int:
    for node in tree.body:
        if not isinstance(node, ast.Assign):
            continue
        if not any(isinstance(target, ast.Name) and target.id == "urlpatterns" for target in node.targets):
            continue
        if not isinstance(node.value, (ast.List, ast.Tuple)):
            continue
        node_start, _ = node_range(starts, node)
        node_aliases = aliases_at_offset(tree, starts, aliases, node_start)
        node_assignments = collect_assignment_roles_until(tree, starts, aliases, node_start)
        for element in node.value.elts:
            if not isinstance(element, ast.Call):
                continue
            raw = dotted_name(element.func)
            if not raw:
                continue
            canonical = canonical_name(raw, node_aliases, node_assignments)
            start, end = node_range(starts, element)
            if canonical == DJANGO_URL_INCLUDE:
                add_fact(
                    facts,
                    unknown_fact(
                        subject_unit_id=module_unit_id,
                        reason_code="FrameworkMagic",
                        affected_claim="python_django_string_dispatch",
                        path=path,
                        content_hash_value=content_hash_value,
                        repository_revision=repository_revision,
                        start=start,
                        end=end,
                    ),
                )
                continue
            if canonical not in DJANGO_URL_CALLS:
                continue
            first_arg = element.args[0] if element.args else None
            path_literal = literal_string_value(first_arg)
            if path_literal is None:
                add_fact(
                    facts,
                    unknown_fact(
                        subject_unit_id=module_unit_id,
                        reason_code="FrameworkMagic",
                        affected_claim="python_django_url_identity",
                        path=path,
                        content_hash_value=content_hash_value,
                        repository_revision=repository_revision,
                        start=start,
                        end=end,
                    ),
                )
                continue
            url_unit = unit(slug(path_literal), "django_url_pattern", start, end, ordinal)
            units.append(url_unit)
            ordinal += 1
            subject_unit_id = unit_id(path, url_unit)
            add_fact(
                facts,
                structural_fact(
                    kind="RESOLVED_CALL",
                    subject_unit_id=subject_unit_id,
                    target=canonical,
                    path=path,
                    content_hash_value=content_hash_value,
                    repository_revision=repository_revision,
                    start=start,
                    end=end,
                    anchor_kind="django_url_route",
                ),
            )
            view_arg = element.args[1] if len(element.args) > 1 else None
            if view_arg is not None and literal_string_value(view_arg) is not None:
                view_start, view_end = node_range(starts, view_arg)
                add_fact(
                    facts,
                    unknown_fact(
                        subject_unit_id=subject_unit_id,
                        reason_code="FrameworkMagic",
                        affected_claim="python_django_string_dispatch",
                        path=path,
                        content_hash_value=content_hash_value,
                        repository_revision=repository_revision,
                        start=view_start,
                        end=view_end,
                    ),
                )
    return ordinal


def collect_pydantic_model_member_facts(
    node: ast.ClassDef,
    starts: list[int],
    path: str,
    content_hash_value: str,
    repository_revision: str,
    subject_unit_id: str,
    aliases: dict[str, str],
    facts: list[dict[str, Any]],
) -> None:
    if not is_pydantic_model(node, aliases):
        return
    for item in node.body:
        if isinstance(item, ast.AnnAssign) and isinstance(item.target, ast.Name):
            field_name = item.target.id
            if field_name == "model_config":
                add_pydantic_model_config_fact(
                    item.target,
                    item.value,
                    starts,
                    path,
                    content_hash_value,
                    repository_revision,
                    subject_unit_id,
                    aliases,
                    facts,
                )
                continue
            if is_python_identifier(field_name):
                start, end = node_range(starts, item.target)
                add_fact(
                    facts,
                    structural_fact(
                        kind="SYMBOL",
                        subject_unit_id=subject_unit_id,
                        target=f"pydantic.field.{field_name}",
                        path=path,
                        content_hash_value=content_hash_value,
                        repository_revision=repository_revision,
                        start=start,
                        end=end,
                        anchor_kind="pydantic_field",
                        ),
                    )
                if isinstance(item.value, ast.Call):
                    value_name = dotted_name(item.value.func)
                    canonical_value = canonical_name(value_name, aliases, {}) if value_name else None
                    if canonical_value == "pydantic.Field":
                        field_start, field_end = node_range(starts, item.value)
                        add_fact(
                            facts,
                            structural_fact(
                                kind="RESOLVED_CALL",
                                subject_unit_id=subject_unit_id,
                                target="pydantic.Field",
                                path=path,
                                content_hash_value=content_hash_value,
                                repository_revision=repository_revision,
                                start=field_start,
                                end=field_end,
                                anchor_kind="pydantic_field_metadata",
                            ),
                        )
            annotation = static_type_name(item.annotation)
            if annotation:
                canonical_annotation = canonical_name(annotation, aliases, {})
                target = f"pydantic.field_type.{canonical_annotation}"
                if is_safe_fact_target(target) and len(target) <= MAX_RUST_PARSE_FACT_TARGET_CHARS:
                    start, end = node_range(starts, item.annotation)
                    add_fact(
                        facts,
                        structural_fact(
                            kind="TYPE",
                            subject_unit_id=subject_unit_id,
                            target=target,
                            path=path,
                            content_hash_value=content_hash_value,
                            repository_revision=repository_revision,
                            start=start,
                            end=end,
                            anchor_kind="pydantic_field_type",
                        ),
                    )
        elif isinstance(item, ast.Assign):
            for target in item.targets:
                if isinstance(target, ast.Name) and target.id == "model_config":
                    add_pydantic_model_config_fact(
                        target,
                        item.value,
                        starts,
                        path,
                        content_hash_value,
                        repository_revision,
                        subject_unit_id,
                        aliases,
                        facts,
                    )
        elif isinstance(item, ast.ClassDef) and item.name == "Config":
            start, end = node_range(starts, item)
            add_fact(
                facts,
                structural_fact(
                    kind="SYMBOL",
                    subject_unit_id=subject_unit_id,
                    target="pydantic.Config",
                    path=path,
                    content_hash_value=content_hash_value,
                    repository_revision=repository_revision,
                    start=start,
                    end=end,
                    anchor_kind="pydantic_config_class",
                ),
            )


def add_pydantic_model_config_fact(
    target: ast.AST,
    value: ast.AST | None,
    starts: list[int],
    path: str,
    content_hash_value: str,
    repository_revision: str,
    subject_unit_id: str,
    aliases: dict[str, str],
    facts: list[dict[str, Any]],
) -> None:
    start, end = node_range(starts, target)
    add_fact(
        facts,
        structural_fact(
            kind="SYMBOL",
            subject_unit_id=subject_unit_id,
            target="pydantic.model_config",
            path=path,
            content_hash_value=content_hash_value,
            repository_revision=repository_revision,
            start=start,
            end=end,
            anchor_kind="pydantic_model_config",
        ),
    )
    if is_dynamic_pydantic_model_config_value(value, aliases):
        value_start, value_end = node_range(starts, value)
        add_fact(
            facts,
            unknown_fact(
                subject_unit_id=subject_unit_id,
                reason_code="FrameworkMagic",
                affected_claim="python_framework_identity",
                path=path,
                content_hash_value=content_hash_value,
                repository_revision=repository_revision,
                start=value_start,
                end=value_end,
            ),
        )


def is_dynamic_pydantic_model_config_value(
    value: ast.AST | None,
    aliases: dict[str, str],
) -> bool:
    if value is None:
        return False
    if is_static_config_literal(value):
        return False
    if is_static_pydantic_config_dict_call(value, aliases):
        return False
    return True


def is_static_pydantic_config_dict_call(value: ast.AST, aliases: dict[str, str]) -> bool:
    if not isinstance(value, ast.Call):
        return False
    name = dotted_name(value.func)
    if not name or canonical_name(name, aliases, {}) != "pydantic.ConfigDict":
        return False
    return not value.args and all(
        keyword.arg is not None and is_static_config_literal(keyword.value)
        for keyword in value.keywords
    )


def is_static_config_literal(value: ast.AST) -> bool:
    if isinstance(value, ast.Constant):
        return True
    if isinstance(value, (ast.List, ast.Set, ast.Tuple)):
        return all(is_static_config_literal(item) for item in value.elts)
    if isinstance(value, ast.Dict):
        return all(
            key is not None
            and is_static_config_literal(key)
            and is_static_config_literal(item_value)
            for key, item_value in zip(value.keys, value.values)
        )
    return False


def sqlalchemy_relationship_target_node(call: ast.Call) -> ast.AST | None:
    if call.args:
        return call.args[0]
    for keyword in call.keywords:
        if keyword.arg == "argument":
            return keyword.value
    return None


def add_sqlalchemy_relationship_target_fact(
    call: ast.Call,
    local_class_names: set[str],
    starts: list[int],
    path: str,
    content_hash_value: str,
    repository_revision: str,
    subject_unit_id: str,
    facts: list[dict[str, Any]],
) -> None:
    target_node = sqlalchemy_relationship_target_node(call)
    if target_node is None:
        return
    start, end = node_range(starts, target_node)
    target_value = literal_string_value(target_node)
    if target_value is None:
        add_fact(
            facts,
            unknown_fact(
                subject_unit_id=subject_unit_id,
                reason_code="FrameworkMagic",
                affected_claim="sqlalchemy_relationship_target",
                path=path,
                content_hash_value=content_hash_value,
                repository_revision=repository_revision,
                start=start,
                end=end,
            ),
        )
        return
    if not is_python_identifier(target_value) or target_value not in local_class_names:
        add_fact(
            facts,
            unknown_fact(
                subject_unit_id=subject_unit_id,
                reason_code="UnresolvedImport",
                affected_claim="sqlalchemy_relationship_target",
                path=path,
                content_hash_value=content_hash_value,
                repository_revision=repository_revision,
                start=start,
                end=end,
            ),
        )
        return
    fact_data = structural_fact(
        kind="SYMBOL",
        subject_unit_id=subject_unit_id,
        target=f"sqlalchemy.relationship_target.{target_value}",
        path=path,
        content_hash_value=content_hash_value,
        repository_revision=repository_revision,
        start=start,
        end=end,
        anchor_kind="sqlalchemy_relationship_target",
    )
    fact_data["assumptions"].extend(
        [
            "fact_scope=context_only",
            "relationship_target_binding=local_literal",
        ]
    )
    add_fact(facts, fact_data)


def collect_sqlalchemy_model_field_facts(
    node: ast.ClassDef,
    local_class_names: set[str],
    starts: list[int],
    path: str,
    content_hash_value: str,
    repository_revision: str,
    subject_unit_id: str,
    aliases: dict[str, str],
    facts: list[dict[str, Any]],
) -> None:
    for item in node.body:
        if not isinstance(item, (ast.AnnAssign, ast.Assign)):
            continue
        annotation = dotted_name(item.annotation) if isinstance(item, ast.AnnAssign) else None
        value = getattr(item, "value", None)
        value_name = dotted_name(value) if isinstance(value, ast.AST) else None
        if annotation:
            canonical_annotation = canonical_name(annotation, aliases, {})
            if canonical_annotation == "sqlalchemy.orm.Mapped":
                start, end = node_range(starts, item.annotation)
                add_fact(
                    facts,
                    structural_fact(
                        kind="TYPE",
                        subject_unit_id=subject_unit_id,
                        target=canonical_annotation,
                        path=path,
                        content_hash_value=content_hash_value,
                        repository_revision=repository_revision,
                        start=start,
                        end=end,
                        anchor_kind="sqlalchemy_mapped_field",
                    ),
                )
        if value_name:
            canonical_value = canonical_name(value_name, aliases, {})
            if canonical_value in {
                "sqlalchemy.orm.mapped_column",
                "sqlalchemy.orm.relationship",
            }:
                start, end = node_range(starts, value if isinstance(value, ast.AST) else item)
                anchor_kind = (
                    "sqlalchemy_relationship"
                    if canonical_value == "sqlalchemy.orm.relationship"
                    else "sqlalchemy_mapped_column"
                )
                add_fact(
                    facts,
                    structural_fact(
                        kind="RESOLVED_CALL",
                        subject_unit_id=subject_unit_id,
                        target=canonical_value,
                        path=path,
                        content_hash_value=content_hash_value,
                        repository_revision=repository_revision,
                        start=start,
                        end=end,
                        anchor_kind=anchor_kind,
                    ),
                )
                if canonical_value == "sqlalchemy.orm.relationship" and isinstance(value, ast.Call):
                    add_sqlalchemy_relationship_target_fact(
                        value,
                        local_class_names,
                        starts,
                        path,
                        content_hash_value,
                        repository_revision,
                        subject_unit_id,
                        facts,
                    )


def canonical_call_name(
    name: str | None,
    aliases: dict[str, str],
    assignments: dict[str, str],
    call_aliases: dict[str, str],
) -> str | None:
    if not name:
        return None
    canonical = canonical_name(name, aliases, assignments)
    parts = canonical.split(".")
    if parts and parts[0] in call_aliases:
        return ".".join([call_aliases[parts[0]], *parts[1:]])
    return canonical


def is_dynamic_namespace_source(node: ast.AST, namespace_aliases: set[str]) -> bool:
    if isinstance(node, ast.Name):
        return node.id in namespace_aliases
    if isinstance(node, ast.Call):
        return dotted_name(node.func) in DYNAMIC_NAMESPACE_FUNCTIONS
    return False


def is_dynamic_namespace_lookup(node: ast.AST, namespace_aliases: set[str]) -> bool:
    if isinstance(node, ast.Subscript):
        return is_dynamic_namespace_source(node.value, namespace_aliases)
    if (
        isinstance(node, ast.Call)
        and isinstance(node.func, ast.Attribute)
        and node.func.attr in {"get", "__getitem__"}
    ):
        return is_dynamic_namespace_source(node.func.value, namespace_aliases)
    return False


def is_dynamic_call(node: ast.Call, namespace_aliases: set[str] | None = None) -> bool:
    namespace_aliases = namespace_aliases or set()
    if isinstance(node.func, ast.Call) and dotted_name(node.func) == "getattr":
        return True
    if isinstance(node.func, ast.Call) and is_dynamic_namespace_lookup(node.func, namespace_aliases):
        return True
    if isinstance(node.func, ast.Subscript):
        return is_dynamic_namespace_lookup(node.func, namespace_aliases)
    return False


def is_dynamic_execution_call(canonical: str | None) -> bool:
    return canonical in {"eval", "exec", "compile"}


def is_monkey_patch_call(node: ast.Call, canonical: str | None) -> bool:
    if canonical != "setattr":
        return False
    if not node.args:
        return True
    target = dotted_name(node.args[0])
    return target not in {"self", "cls"}


def scope_name_bindings(child: ast.AST) -> list[tuple[str, str | None]]:
    """Names bound by one AST node, paired with a string-literal value only for a
    plain single-target `name = "literal"` assignment (else `None`). Every other
    binding form (annotated/augmented assignment, function parameter, loop, with,
    except, comprehension target) contributes a binding with no constant value so
    it can poison a name against constant propagation."""
    bindings: list[tuple[str, str | None]] = []
    if isinstance(child, ast.Assign):
        constant_value = (
            child.value.value
            if len(child.targets) == 1
            and isinstance(child.targets[0], ast.Name)
            and isinstance(child.value, ast.Constant)
            and isinstance(child.value.value, str)
            else None
        )
        for target in child.targets:
            for name in binding_target_names(target):
                bindings.append((name, constant_value if isinstance(target, ast.Name) else None))
    elif isinstance(child, (ast.AnnAssign, ast.AugAssign)):
        if isinstance(child.target, ast.Name):
            bindings.append((child.target.id, None))
    elif isinstance(child, (ast.FunctionDef, ast.AsyncFunctionDef)):
        args = child.args
        for argument in [*args.posonlyargs, *args.args, *args.kwonlyargs]:
            bindings.append((argument.arg, None))
        if args.vararg is not None:
            bindings.append((args.vararg.arg, None))
        if args.kwarg is not None:
            bindings.append((args.kwarg.arg, None))
    elif isinstance(child, (ast.For, ast.AsyncFor)):
        bindings.extend((name, None) for name in binding_target_names(child.target))
    elif isinstance(child, (ast.With, ast.AsyncWith)):
        for item in child.items:
            if item.optional_vars is not None:
                bindings.extend((name, None) for name in binding_target_names(item.optional_vars))
    elif isinstance(child, ast.ExceptHandler):
        if child.name:
            bindings.append((child.name, None))
    elif isinstance(child, ast.comprehension):
        bindings.extend((name, None) for name in binding_target_names(child.target))
    return bindings


def binding_target_names(target: ast.AST) -> list[str]:
    if isinstance(target, ast.Name):
        return [target.id]
    if isinstance(target, ast.Starred):
        return binding_target_names(target.value)
    if isinstance(target, (ast.Tuple, ast.List)):
        names: list[str] = []
        for element in target.elts:
            names.extend(binding_target_names(element))
        return names
    return []


def collect_safe_string_constants(node: ast.AST) -> dict[str, str]:
    """Sound intra-scope string-constant propagation. A name qualifies only when
    it is bound exactly once in the scope subtree, via a plain
    `name = "literal"` assignment. Any name bound more than once, reassigned,
    parameter-bound, or bound by a non-plain form is excluded, so an ambiguous
    or reassigned name stays UNKNOWN rather than resolving to a guessed value."""
    binding_counts: dict[str, int] = {}
    constant_values: dict[str, str] = {}
    for child in ast.walk(node):
        for name, value in scope_name_bindings(child):
            binding_counts[name] = binding_counts.get(name, 0) + 1
            if value is not None:
                constant_values[name] = value
    return {
        name: value
        for name, value in constant_values.items()
        if binding_counts.get(name, 0) == 1
    }


def resolved_dynamic_import_literal_target(
    call: ast.Call,
    module_index: dict[str, list[str]] | None,
    string_constants: dict[str, str] | None = None,
) -> str | None:
    first_arg = call.args[0] if call.args else None
    literal_value: str | None = None
    if isinstance(first_arg, ast.Constant) and isinstance(first_arg.value, str):
        literal_value = first_arg.value
    elif isinstance(first_arg, ast.Name) and string_constants is not None:
        # Sound constant propagation: a single-static string constant assigned in
        # the same scope is equivalent to writing the literal at the call site.
        literal_value = string_constants.get(first_arg.id)
    if (
        literal_value is not None
        and is_safe_fact_target(literal_value)
        and repo_local_module_resolution(literal_value, module_index) == "resolved"
    ):
        return literal_value
    return None


def import_builtin_is_relative_or_ambiguous(call: ast.Call) -> bool:
    """`__import__(name, globals, locals, fromlist, level)` performs a relative
    import when `level` is a nonzero int, so the first argument no longer names
    an absolute module. Return True when the call could be relative or when its
    argument shape hides a `level` (a positional `*args` splat, a `**kwargs`
    splat, a `level` argument that is not a literal `0`), so those calls abstain
    to a typed UNKNOWN instead of resolving the name as an absolute module."""
    if any(isinstance(arg, ast.Starred) for arg in call.args):
        return True
    level_arg: ast.AST | None = call.args[4] if len(call.args) >= 5 else None
    for keyword in call.keywords:
        if keyword.arg is None:
            return True
        if keyword.arg == "level":
            level_arg = keyword.value
    if level_arg is None:
        return False
    return not (isinstance(level_arg, ast.Constant) and level_arg.value == 0)


def dynamic_unknown_for_call(
    call: ast.Call,
    canonical: str | None,
    module_index: dict[str, list[str]] | None,
    namespace_aliases: set[str] | None = None,
    dynamic_value_aliases: dict[str, tuple[str, str]] | None = None,
    aliases: dict[str, str] | None = None,
    assignments: dict[str, str] | None = None,
    string_constants: dict[str, str] | None = None,
) -> tuple[str, str] | None:
    namespace_aliases = namespace_aliases or set()
    dynamic_value_aliases = dynamic_value_aliases or {}
    aliases = aliases or {}
    assignments = assignments or {}
    if canonical:
        root = canonical.split(".", 1)[0]
        if root in dynamic_value_aliases:
            return dynamic_value_aliases[root]
    if is_monkey_patch_call(call, canonical):
        return "MonkeyPatch", "python_call_target"
    if canonical in {"sys.path.append", "sys.path.insert"}:
        return "RuntimeDependencyInjection", "python_import_resolution"
    if canonical == "__import__" and (
        import_builtin_is_relative_or_ambiguous(call)
        or resolved_dynamic_import_literal_target(call, module_index, string_constants) is None
    ):
        return "DynamicImport", "python_import_resolution"
    if (
        canonical == "importlib.import_module"
        and resolved_dynamic_import_literal_target(call, module_index, string_constants) is None
    ):
        return "DynamicImport", "python_import_resolution"
    if canonical in SQLALCHEMY_EVENT_LISTENER_CALLS:
        return "FrameworkMagic", "python_framework_identity"
    if is_dynamic_sqlalchemy_model_class_call(call, canonical, aliases, assignments):
        return "FrameworkMagic", "python_framework_identity"
    if canonical in DYNAMIC_NAMESPACE_FUNCTIONS:
        return "FrameworkMagic", "python_call_target"
    if is_dynamic_execution_call(canonical):
        return "FrameworkMagic", "python_call_target"
    if is_dynamic_call(call, namespace_aliases):
        return "FrameworkMagic", "python_call_target"
    return None


def is_dynamic_sqlalchemy_model_class_call(
    call: ast.Call,
    canonical: str | None,
    aliases: dict[str, str],
    assignments: dict[str, str],
) -> bool:
    if canonical != "type" or len(call.args) < 2:
        return False
    bases = call.args[1]
    if not isinstance(bases, (ast.List, ast.Tuple)):
        return False
    for base in bases.elts:
        name = dotted_name(base)
        if name and canonical_name(name, aliases, assignments) in SQLALCHEMY_MODEL_BASES:
            return True
    return False


def is_sqlalchemy_runtime_session_injection_call(
    canonical: str | None,
    unit_kind: str,
) -> bool:
    if unit_kind != "sqlalchemy_repository_method" or not canonical:
        return False
    parts = canonical.split(".")
    return len(parts) >= 3 and parts[0] == "self" and parts[-1] in SQLALCHEMY_SESSION_METHODS


def dynamic_call_alias_target(
    value: ast.AST | None,
    aliases: dict[str, str],
    assignments: dict[str, str],
    call_aliases: dict[str, str],
) -> str | None:
    if value is None:
        return None
    name = static_reference_name(value)
    target = canonical_call_name(name, aliases, assignments, call_aliases)
    if target in DYNAMIC_CALL_ALIAS_TARGETS:
        return target
    return None


def dynamic_namespace_assignment(
    value: ast.AST | None,
    aliases: dict[str, str],
    assignments: dict[str, str],
    call_aliases: dict[str, str],
) -> bool:
    if not isinstance(value, ast.Call):
        return False
    name = dotted_name(value.func)
    target = canonical_call_name(name, aliases, assignments, call_aliases)
    return target in DYNAMIC_NAMESPACE_FUNCTIONS


def dynamic_value_unknown_assignment(
    value: ast.AST | None,
    aliases: dict[str, str],
    assignments: dict[str, str],
    call_aliases: dict[str, str],
    namespace_aliases: set[str],
) -> tuple[str, str] | None:
    if value is None:
        return None
    if is_dynamic_namespace_lookup(value, namespace_aliases):
        return "FrameworkMagic", "python_call_target"
    if isinstance(value, ast.Call):
        name = dotted_name(value.func)
        target = canonical_call_name(name, aliases, assignments, call_aliases)
        if target == "getattr":
            return "FrameworkMagic", "python_call_target"
    return None


def update_dynamic_bindings_from_assignment(
    event: ast.Assign | ast.AnnAssign | ast.AugAssign,
    aliases: dict[str, str],
    assignments: dict[str, str],
    call_aliases: dict[str, str],
    namespace_aliases: set[str],
    dynamic_value_aliases: dict[str, tuple[str, str]],
) -> None:
    value = event.value if isinstance(event, (ast.Assign, ast.AnnAssign)) else None
    call_alias = dynamic_call_alias_target(value, aliases, assignments, call_aliases)
    is_namespace_alias = dynamic_namespace_assignment(value, aliases, assignments, call_aliases)
    value_unknown = dynamic_value_unknown_assignment(
        value,
        aliases,
        assignments,
        call_aliases,
        namespace_aliases,
    )
    for target in assignment_target_names(event):
        call_aliases.pop(target, None)
        namespace_aliases.discard(target)
        dynamic_value_aliases.pop(target, None)
        if call_alias is not None:
            call_aliases[target] = call_alias
        if is_namespace_alias:
            namespace_aliases.add(target)
        if value_unknown is not None:
            dynamic_value_aliases[target] = value_unknown


def collect_dynamic_bindings_until(
    tree: ast.Module,
    starts: list[int],
    aliases: dict[str, str],
    offset: int,
) -> tuple[dict[str, str], set[str], dict[str, tuple[str, str]]]:
    dynamic_call_aliases: dict[str, str] = {}
    namespace_aliases: set[str] = set()
    dynamic_value_aliases: dict[str, tuple[str, str]] = {}
    for item in tree.body:
        item_start, _item_end = node_range(starts, item)
        if item_start >= offset:
            break
        if isinstance(item, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef)):
            continue
        events = [
            child
            for child in ast.walk(item)
            if isinstance(child, (ast.Assign, ast.AnnAssign, ast.AugAssign))
        ]
        events.sort(key=lambda child: node_range(starts, child))
        for event in events:
            event_start, _event_end = node_range(starts, event)
            if event_start >= offset:
                continue
            event_aliases = aliases_at_offset(tree, starts, aliases, event_start)
            event_assignments = collect_assignment_roles_until(tree, starts, aliases, event_start)
            update_dynamic_bindings_from_assignment(
                event,
                event_aliases,
                event_assignments,
                dynamic_call_aliases,
                namespace_aliases,
                dynamic_value_aliases,
            )
    return dynamic_call_aliases, namespace_aliases, dynamic_value_aliases


def add_unknown_fact(
    facts: list[dict[str, Any]],
    subject_unit_id: str,
    reason_code: str,
    affected_claim: str,
    path: str,
    content_hash_value: str,
    repository_revision: str,
    start: int,
    end: int,
) -> None:
    add_fact(
        facts,
        unknown_fact(
            subject_unit_id=subject_unit_id,
            reason_code=reason_code,
            affected_claim=affected_claim,
            path=path,
            content_hash_value=content_hash_value,
            repository_revision=repository_revision,
            start=start,
            end=end,
        ),
    )


def module_level_dynamic_unknown_specs(
    tree: ast.Module,
    starts: list[int],
    aliases: dict[str, str],
    assignments: dict[str, str],
    module_index: dict[str, list[str]] | None,
) -> list[tuple[str, str, int, int]]:
    specs: list[tuple[str, str, int, int]] = []
    dynamic_call_aliases: dict[str, str] = {}
    namespace_aliases: set[str] = set()
    dynamic_value_aliases: dict[str, tuple[str, str]] = {}
    for item in tree.body:
        if isinstance(item, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef)):
            continue
        events = [
            child
            for child in ast.walk(item)
            if isinstance(child, (ast.Assign, ast.AnnAssign, ast.AugAssign, ast.Call))
        ]
        events.sort(key=lambda child: (node_range(starts, child), 0 if not isinstance(child, ast.Call) else 1))
        for event in events:
            start, end = node_range(starts, event)
            event_aliases = aliases_at_offset(tree, starts, aliases, start)
            call_assignments = collect_assignment_roles_until(tree, starts, aliases, start)
            if not isinstance(event, ast.Call):
                update_dynamic_bindings_from_assignment(
                    event,
                    event_aliases,
                    call_assignments,
                    dynamic_call_aliases,
                    namespace_aliases,
                    dynamic_value_aliases,
                )
                continue
            call = event
            name = dotted_name(call.func)
            canonical = canonical_call_name(name, event_aliases, call_assignments, dynamic_call_aliases)
            unknown = dynamic_unknown_for_call(
                call,
                canonical,
                module_index,
                namespace_aliases,
                dynamic_value_aliases,
                aliases=event_aliases,
                assignments=call_assignments,
            )
            if unknown is None:
                continue
            reason_code, affected_claim = unknown
            specs.append((reason_code, affected_claim, start, end))
    return specs


def add_unknown_specs_for_unit(
    facts: list[dict[str, Any]],
    subject_unit_id: str,
    specs: list[tuple[str, str, int, int]],
    path: str,
    content_hash_value: str,
    repository_revision: str,
    evidence_range: tuple[int, int] | None = None,
) -> None:
    for reason_code, affected_claim, start, end in specs:
        if evidence_range is not None and start > evidence_range[0]:
            continue
        fact_start, fact_end = evidence_range or (start, end)
        add_unknown_fact(
            facts,
            subject_unit_id,
            reason_code,
            affected_claim,
            path,
            content_hash_value,
            repository_revision,
            fact_start,
            fact_end,
        )


def collect_call_facts(
    node: ast.AST,
    unit_kind: str,
    starts: list[int],
    path: str,
    content_hash_value: str,
    repository_revision: str,
    subject_unit_id: str,
    aliases: dict[str, str],
    assignments: dict[str, str],
    parameter_roles: dict[str, str] | None,
    defined_names: set[str],
    module_index: dict[str, list[str]] | None,
    facts: list[dict[str, Any]],
    initial_dynamic_call_aliases: dict[str, str] | None = None,
    initial_namespace_aliases: set[str] | None = None,
    initial_dynamic_value_aliases: dict[str, tuple[str, str]] | None = None,
    custom_query_wrappers: set[str] | None = None,
) -> None:
    parameter_roles = parameter_roles or {}
    custom_query_wrappers = custom_query_wrappers or set()
    string_constants = collect_safe_string_constants(node)
    shadowed_receivers = assigned_role_receivers(node)
    pydantic_validator_body_start = (
        function_body_start_byte(node, starts)
        if has_pydantic_runtime_validator_decorator(node, aliases, assignments)
        else None
    )
    events = [
        child
        for child in ast.walk(node)
        if isinstance(child, (ast.Assign, ast.AnnAssign, ast.AugAssign, ast.Call))
    ]
    events.sort(key=lambda child: (node_range(starts, child), 0 if not isinstance(child, ast.Call) else 1))
    local_assignments = dict(assignments)
    dynamic_call_aliases = dict(initial_dynamic_call_aliases or {})
    namespace_aliases = set(initial_namespace_aliases or set())
    dynamic_value_aliases = dict(initial_dynamic_value_aliases or {})
    for event in events:
        if not isinstance(event, ast.Call):
            role = (
                assignment_role(event.value, aliases, local_assignments)
                if isinstance(event, (ast.Assign, ast.AnnAssign)) and isinstance(event.value, ast.AST)
                else None
            )
            for target in assignment_target_names(event):
                if role is None:
                    local_assignments.pop(target, None)
                else:
                    local_assignments[target] = role
            update_dynamic_bindings_from_assignment(
                event,
                aliases,
                local_assignments,
                dynamic_call_aliases,
                namespace_aliases,
                dynamic_value_aliases,
            )
            continue
        call = event
        start, end = node_range(starts, call)
        name = dotted_name(call.func)
        canonical = canonical_call_name(name, aliases, local_assignments, dynamic_call_aliases)
        if canonical:
            parts = canonical.split(".")
            if (
                len(parts) >= 2
                and ".".join(parts[:-1]) in parameter_roles
                and ".".join(parts[:-1]) not in shadowed_receivers
                and parts[-1] in SQLALCHEMY_SESSION_METHODS
            ):
                receiver = ".".join(parts[:-1])
                canonical = f"{parameter_roles[receiver]}.{parts[-1]}"
        unknown = dynamic_unknown_for_call(
            call,
            canonical,
            module_index,
            namespace_aliases,
            dynamic_value_aliases,
            aliases=aliases,
            assignments=local_assignments,
            string_constants=string_constants,
        )
        if unknown is not None:
            reason_code, affected_claim = unknown
            add_unknown_fact(
                facts,
                subject_unit_id,
                reason_code,
                affected_claim,
                path,
                content_hash_value,
                repository_revision,
                start,
                end,
            )
            continue
        if is_sqlalchemy_runtime_session_injection_call(canonical, unit_kind):
            add_unknown_fact(
                facts,
                subject_unit_id,
                "RuntimeDependencyInjection",
                "python_framework_identity",
                path,
                content_hash_value,
                repository_revision,
                start,
                end,
            )
            continue
        if is_sqlalchemy_custom_query_wrapper_call(canonical, custom_query_wrappers):
            add_unknown_fact(
                facts,
                subject_unit_id,
                "FrameworkMagic",
                "python_framework_identity",
                path,
                content_hash_value,
                repository_revision,
                start,
                end,
            )
            continue
        if pydantic_validator_body_start is not None and start >= pydantic_validator_body_start:
            add_unknown_fact(
                facts,
                subject_unit_id,
                "FrameworkMagic",
                "pydantic_validator_side_effects",
                path,
                content_hash_value,
                repository_revision,
                start,
                end,
            )
            continue
        if canonical in {"importlib.import_module", "__import__"}:
            literal_target = resolved_dynamic_import_literal_target(
                call, module_index, string_constants
            )
            if literal_target is not None:
                add_fact(
                    facts,
                    structural_fact(
                        kind="RESOLVED_IMPORT",
                        subject_unit_id=subject_unit_id,
                        target=literal_target,
                        path=path,
                        content_hash_value=content_hash_value,
                        repository_revision=repository_revision,
                        start=start,
                        end=end,
                        anchor_kind="dynamic_import_literal",
                    ),
                )
            continue
        if canonical == "pydantic.create_model":
            add_fact(
                facts,
                unknown_fact(
                    subject_unit_id=subject_unit_id,
                    reason_code="FrameworkMagic",
                    affected_claim="python_framework_identity",
                    path=path,
                    content_hash_value=content_hash_value,
                    repository_revision=repository_revision,
                    start=start,
                    end=end,
                ),
            )
            continue
        if canonical:
            canonical_parts = canonical.split(".")
            add_fact(
                facts,
                structural_fact(
                    kind="RESOLVED_CALL",
                    subject_unit_id=subject_unit_id,
                    target=canonical,
                    path=path,
                    content_hash_value=content_hash_value,
                    repository_revision=repository_revision,
                    start=start,
                    end=end,
                    anchor_kind=call_anchor_kind(canonical, unit_kind),
                ),
            )
            if (
                len(canonical_parts) >= 2
                and ".".join(canonical_parts[:-1]) in SQLALCHEMY_SESSION_TYPES
                and canonical_parts[-1] in SQLALCHEMY_QUERY_METHODS
                and is_sqlalchemy_raw_sql_argument(
                    call.args[0] if call.args else None,
                    aliases,
                    local_assignments,
                )
            ):
                add_sqlalchemy_raw_sql_unknown(
                    call,
                    starts,
                    path,
                    content_hash_value,
                    repository_revision,
                    subject_unit_id,
                    facts,
                )
            if canonical == "fastapi.Depends":
                collect_fastapi_dependency_target_fact(
                    call,
                    starts,
                    path,
                    content_hash_value,
                    repository_revision,
                    subject_unit_id,
                    aliases,
                    local_assignments,
                    defined_names,
                    facts,
                )
            elif canonical == "fastapi.HTTPException":
                collect_fastapi_http_exception_status_fact(
                    call,
                    starts,
                    path,
                    content_hash_value,
                    repository_revision,
                    subject_unit_id,
                    facts,
                )


def collect_module_dynamic_pydantic_model_facts(
    tree: ast.Module,
    starts: list[int],
    path: str,
    content_hash_value: str,
    repository_revision: str,
    subject_unit_id: str,
    aliases: dict[str, str],
    assignments: dict[str, str],
    facts: list[dict[str, Any]],
) -> None:
    for item in tree.body:
        if isinstance(item, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef)):
            continue
        for call in [child for child in ast.walk(item) if isinstance(child, ast.Call)]:
            name = dotted_name(call.func)
            if not name or canonical_name(name, aliases, assignments) != "pydantic.create_model":
                continue
            start, end = node_range(starts, call)
            add_fact(
                facts,
                unknown_fact(
                    subject_unit_id=subject_unit_id,
                    reason_code="FrameworkMagic",
                    affected_claim="python_framework_identity",
                    path=path,
                    content_hash_value=content_hash_value,
                    repository_revision=repository_revision,
                    start=start,
                    end=end,
                ),
            )


def is_sqlalchemy_raw_sql_argument(
    node: ast.AST | None,
    aliases: dict[str, str],
    assignments: dict[str, str],
) -> bool:
    if isinstance(node, ast.Constant) and isinstance(node.value, (str, bytes)):
        return True
    if isinstance(node, ast.Call):
        name = dotted_name(node.func)
        canonical = canonical_name(name, aliases, assignments) if name else None
        return canonical in {"sqlalchemy.text", "sqlalchemy.sql.text"}
    return False


def add_sqlalchemy_raw_sql_unknown(
    call: ast.Call,
    starts: list[int],
    path: str,
    content_hash_value: str,
    repository_revision: str,
    subject_unit_id: str,
    facts: list[dict[str, Any]],
) -> None:
    subject = call.args[0] if call.args else call
    start, end = node_range(starts, subject)
    add_fact(
        facts,
        unknown_fact(
            subject_unit_id=subject_unit_id,
            reason_code="FrameworkMagic",
            affected_claim="sqlalchemy_query_shape",
            path=path,
            content_hash_value=content_hash_value,
            repository_revision=repository_revision,
            start=start,
            end=end,
        ),
    )


def collect_fastapi_dependency_target_fact(
    call: ast.Call,
    starts: list[int],
    path: str,
    content_hash_value: str,
    repository_revision: str,
    subject_unit_id: str,
    aliases: dict[str, str],
    assignments: dict[str, str],
    defined_names: set[str],
    facts: list[dict[str, Any]],
) -> None:
    dependency = call.args[0] if call.args else None
    if dependency is None:
        for keyword in call.keywords:
            if keyword.arg == "dependency":
                dependency = keyword.value
                break
    if dependency is None:
        return
    name = static_reference_name(dependency)
    if not name:
        start, end = node_range(starts, dependency)
        add_fact(
            facts,
            unknown_fact(
                subject_unit_id=subject_unit_id,
                reason_code="RuntimeDependencyInjection",
                affected_claim="fastapi_dependency_target",
                path=path,
                content_hash_value=content_hash_value,
                repository_revision=repository_revision,
                start=start,
                end=end,
            ),
        )
        return
    root_name = name.split(".", 1)[0]
    if root_name not in aliases and root_name not in assignments and root_name not in defined_names:
        start, end = node_range(starts, dependency)
        add_fact(
            facts,
            unknown_fact(
                subject_unit_id=subject_unit_id,
                reason_code="RuntimeDependencyInjection",
                affected_claim="fastapi_dependency_target",
                path=path,
                content_hash_value=content_hash_value,
                repository_revision=repository_revision,
                start=start,
                end=end,
            ),
        )
        return
    target_name = canonical_name(name, aliases, assignments)
    target = f"fastapi.dependency.{target_name}"
    if not is_safe_fact_target(target) or len(target) > MAX_RUST_PARSE_FACT_TARGET_CHARS:
        return
    start, end = node_range(starts, dependency)
    add_fact(
        facts,
        structural_fact(
            kind="SYMBOL",
            subject_unit_id=subject_unit_id,
            target=target,
            path=path,
            content_hash_value=content_hash_value,
            repository_revision=repository_revision,
            start=start,
            end=end,
            anchor_kind="fastapi_dependency_target",
        ),
    )


def collect_fastapi_http_exception_status_fact(
    call: ast.Call,
    starts: list[int],
    path: str,
    content_hash_value: str,
    repository_revision: str,
    subject_unit_id: str,
    facts: list[dict[str, Any]],
) -> None:
    status = call.args[0] if call.args else None
    if status is None:
        for keyword in call.keywords:
            if keyword.arg == "status_code":
                status = keyword.value
                break
    if (
        not isinstance(status, ast.Constant)
        or not isinstance(status.value, int)
        or isinstance(status.value, bool)
    ):
        return
    if status.value < 100 or status.value > 599:
        return
    target = f"fastapi.http_exception.status_code.{status.value}"
    if not is_safe_fact_target(target) or len(target) > MAX_RUST_PARSE_FACT_TARGET_CHARS:
        return
    start, end = node_range(starts, status)
    add_fact(
        facts,
        structural_fact(
            kind="SYMBOL",
            subject_unit_id=subject_unit_id,
            target=target,
            path=path,
            content_hash_value=content_hash_value,
            repository_revision=repository_revision,
            start=start,
            end=end,
            anchor_kind="fastapi_http_exception_status",
        ),
    )


def call_anchor_kind(target: str, unit_kind: str) -> str:
    if target == "fastapi.Depends":
        return "fastapi_dependency"
    if target == "fastapi.HTTPException":
        return "fastapi_http_exception"
    if target == "sqlalchemy.select":
        return "sqlalchemy_select"
    if any(
        target.startswith(f"{session_type}.")
        for session_type in SQLALCHEMY_SESSION_TYPES
    ):
        return "sqlalchemy_session_call"
    if unit_kind == "fastapi_route" and is_application_call_target(target):
        return "fastapi_service_call"
    return "call_target"


def add_pytest_fixture_unknown(
    facts: list[dict[str, Any]],
    subject_unit_id: str,
    reason_code: str,
    path: str,
    content_hash_value: str,
    repository_revision: str,
    start: int,
    end: int,
) -> None:
    add_fact(
        facts,
        unknown_fact(
            subject_unit_id=subject_unit_id,
            reason_code=reason_code,
            affected_claim="pytest_fixture_binding",
            path=path,
            content_hash_value=content_hash_value,
            repository_revision=repository_revision,
            start=start,
            end=end,
        ),
    )


def add_pytest_fixture_context_fact(
    facts: list[dict[str, Any]],
    subject_unit_id: str,
    target: str,
    anchor_kind: str,
    path: str,
    content_hash_value: str,
    repository_revision: str,
    start: int,
    end: int,
) -> None:
    if anchor_kind in {"pytest_fixture_edge", "pytest_conftest_fixture_edge"}:
        add_fact(
            facts,
            dataflow_derived_fact(
                kind="SYMBOL",
                subject_unit_id=subject_unit_id,
                target=target,
                path=path,
                content_hash_value=content_hash_value,
                repository_revision=repository_revision,
                start=start,
                end=end,
                anchor_kind=anchor_kind,
                derived_from=PYTEST_FIXTURE_GRAPH,
            ),
        )
        return
    add_fact(
        facts,
        structural_fact(
            kind="SYMBOL",
            subject_unit_id=subject_unit_id,
            target=target,
            path=path,
            content_hash_value=content_hash_value,
            repository_revision=repository_revision,
            start=start,
            end=end,
            anchor_kind=anchor_kind,
        ),
    )


def add_pytest_fixture_binding_for_name(
    facts: list[dict[str, Any]],
    subject_unit_id: str,
    fixture_name: str,
    fixture_name_counts: dict[str, int],
    conftest_fixture_name_counts: dict[str, int],
    path: str,
    content_hash_value: str,
    repository_revision: str,
    start: int,
    end: int,
) -> None:
    if fixture_name_counts.get(fixture_name, 0) == 1:
        add_pytest_fixture_context_fact(
            facts,
            subject_unit_id,
            f"pytest.fixture.{fixture_name}",
            "pytest_fixture_edge",
            path,
            content_hash_value,
            repository_revision,
            start,
            end,
        )
    elif fixture_name_counts.get(fixture_name, 0) > 1:
        add_pytest_fixture_unknown(
            facts,
            subject_unit_id,
            "ConflictingFacts",
            path,
            content_hash_value,
            repository_revision,
            start,
            end,
        )
    elif conftest_fixture_name_counts.get(fixture_name, 0) == 1:
        add_pytest_fixture_context_fact(
            facts,
            subject_unit_id,
            f"pytest.fixture.{fixture_name}",
            "pytest_conftest_fixture_edge",
            path,
            content_hash_value,
            repository_revision,
            start,
            end,
        )
    elif conftest_fixture_name_counts.get(fixture_name, 0) > 1:
        add_pytest_fixture_unknown(
            facts,
            subject_unit_id,
            "ConflictingFacts",
            path,
            content_hash_value,
            repository_revision,
            start,
            end,
        )
    elif fixture_name in PYTEST_BUILTIN_FIXTURES:
        add_pytest_fixture_context_fact(
            facts,
            subject_unit_id,
            f"pytest.builtin_fixture.{fixture_name}",
            "pytest_builtin_fixture_context",
            path,
            content_hash_value,
            repository_revision,
            start,
            end,
        )
    elif fixture_name in PYTEST_PLUGIN_FIXTURES:
        add_pytest_fixture_context_fact(
            facts,
            subject_unit_id,
            f"pytest.plugin_fixture.{fixture_name}",
            "pytest_plugin_fixture_context",
            path,
            content_hash_value,
            repository_revision,
            start,
            end,
        )
    else:
        add_pytest_fixture_unknown(
            facts,
            subject_unit_id,
            "PytestFixtureInjection",
            path,
            content_hash_value,
            repository_revision,
            start,
            end,
        )


def literal_getfixturevalue_name(call: ast.Call) -> str | None:
    if dotted_name(call.func) != "request.getfixturevalue" or not call.args:
        return None
    first = call.args[0]
    if isinstance(first, ast.Constant) and isinstance(first.value, str) and is_python_identifier(first.value):
        return first.value
    return None


def collect_fixture_facts(
    node: ast.FunctionDef | ast.AsyncFunctionDef,
    starts: list[int],
    path: str,
    content_hash_value: str,
    repository_revision: str,
    subject_unit_id: str,
    fixture_name_counts: dict[str, int],
    conftest_fixture_name_counts: dict[str, int] | None,
    parametrize_names: set[str] | None,
    indirect_parametrize_names: set[str] | None,
    aliases: dict[str, str],
    assignments: dict[str, str],
    facts: list[dict[str, Any]],
) -> None:
    is_test_function = node.name.startswith("test_")
    is_fixture_function = has_pytest_fixture_decorator(node, aliases, assignments)
    if not is_test_function and not is_fixture_function:
        return
    start, end = node_range(starts, node)
    if is_test_function:
        add_fact(
            facts,
            structural_fact(
                kind="SYMBOL",
                subject_unit_id=subject_unit_id,
                target="pytest.test",
                path=path,
                content_hash_value=content_hash_value,
                repository_revision=repository_revision,
                start=start,
                end=end,
                anchor_kind="pytest_test_function",
            ),
        )
    conftest_fixture_name_counts = conftest_fixture_name_counts or {}
    parametrize_names = parametrize_names if is_test_function else set()
    indirect_parametrize_names = indirect_parametrize_names if is_test_function else set()
    parametrize_names = parametrize_names or set()
    indirect_parametrize_names = indirect_parametrize_names or set()
    for arg in node.args.args:
        if arg.arg == "self":
            continue
        start, end = node_range(starts, arg)
        if arg.arg in parametrize_names:
            add_fact(
                facts,
                structural_fact(
                    kind="SYMBOL",
                    subject_unit_id=subject_unit_id,
                    target=f"pytest.parametrize.{arg.arg}",
                    path=path,
                    content_hash_value=content_hash_value,
                    repository_revision=repository_revision,
                    start=start,
                    end=end,
                    anchor_kind="pytest_parametrize_arg",
                ),
            )
        elif arg.arg in indirect_parametrize_names:
            add_pytest_fixture_unknown(
                facts,
                subject_unit_id,
                "PytestFixtureInjection",
                path,
                content_hash_value,
                repository_revision,
                start,
                end,
            )
        else:
            add_pytest_fixture_binding_for_name(
                facts,
                subject_unit_id,
                arg.arg,
                fixture_name_counts,
                conftest_fixture_name_counts,
                path,
                content_hash_value,
                repository_revision,
                start,
                end,
            )
    for call in ast.walk(node):
        if not isinstance(call, ast.Call) or dotted_name(call.func) != "request.getfixturevalue":
            continue
        start, end = node_range(starts, call)
        fixture_name = literal_getfixturevalue_name(call)
        if fixture_name is None:
            add_pytest_fixture_unknown(
                facts,
                subject_unit_id,
                "PytestFixtureInjection",
                path,
                content_hash_value,
                repository_revision,
                start,
                end,
            )
            continue
        add_pytest_fixture_binding_for_name(
            facts,
            subject_unit_id,
            fixture_name,
            fixture_name_counts,
            conftest_fixture_name_counts,
            path,
            content_hash_value,
            repository_revision,
            start,
            end,
        )


def analyze_source(
    path: str,
    source: str,
    content_hash_value: str,
    repository_revision: str,
    module_index: dict[str, list[str]] | None = None,
    source_roots: list[str] | None = None,
    conftest_fixture_name_counts: dict[str, int] | None = None,
    module_symbols: dict[str, dict[str, tuple[str, str]]] | None = None,
    module_all_names: dict[str, set[str]] | None = None,
) -> tuple[list[dict[str, Any]], list[dict[str, Any]], list[dict[str, Any]]]:
    starts = byte_line_starts(source)
    units: list[dict[str, Any]] = []
    diagnostics: list[dict[str, Any]] = []
    facts: list[dict[str, Any]] = []

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
        return units, diagnostics, facts

    ordinal = 0
    units.append(unit("module", "module", 0, len(source.encode("utf-8")), ordinal))
    module_unit_id = unit_id(path, units[0])
    ordinal += 1
    unit_by_node: dict[int, dict[str, Any]] = {}
    aliases, import_facts = collect_import_aliases(
        tree,
        starts,
        path,
        content_hash_value,
        repository_revision,
        module_unit_id,
        module_index,
        source_roots or [],
        module_symbols,
        module_all_names,
    )
    for item in import_facts:
        add_fact(facts, item)
    defined_names = top_level_defined_names(tree)
    local_class_names = {item.name for item in tree.body if isinstance(item, ast.ClassDef)}
    collect_module_identity_and_scope_facts(
        tree,
        path,
        source,
        content_hash_value,
        repository_revision,
        module_unit_id,
        facts,
    )
    source_end = len(source.encode("utf-8"))
    assignments = collect_assignment_roles_until(tree, starts, aliases, source_end + 1)
    module_dynamic_unknowns = module_level_dynamic_unknown_specs(
        tree,
        starts,
        aliases,
        assignments,
        module_index,
    )
    add_unknown_specs_for_unit(
        facts,
        module_unit_id,
        module_dynamic_unknowns,
        path,
        content_hash_value,
        repository_revision,
    )
    collect_module_dynamic_pydantic_model_facts(
        tree,
        starts,
        path,
        content_hash_value,
        repository_revision,
        module_unit_id,
        aliases,
        assignments,
        facts,
    )
    collect_fastapi_include_router_facts(
        tree,
        starts,
        path,
        content_hash_value,
        repository_revision,
        module_unit_id,
        aliases,
        assignments,
        module_index,
        facts,
    )
    fixture_name_counts = pytest_fixture_name_counts_from_tree(tree, aliases, assignments)
    module_custom_query_wrappers = collect_sqlalchemy_custom_query_wrapper_names(tree, aliases)

    for item in tree.body:
        if isinstance(item, (ast.FunctionDef, ast.AsyncFunctionDef)):
            start, end = node_range(starts, item)
            item_aliases = aliases_at_offset(tree, starts, aliases, start)
            item_assignments = collect_assignment_roles_until(tree, starts, aliases, start)
            units.append(
                unit(
                    item.name,
                    function_kind(item, None, item_aliases, item_assignments),
                    start,
                    end,
                    ordinal,
                )
            )
            unit_by_node[id(item)] = units[-1]
            ordinal += 1
        elif isinstance(item, ast.ClassDef):
            start, end = node_range(starts, item)
            item_aliases = aliases_at_offset(tree, starts, aliases, start)
            item_assignments = collect_assignment_roles_until(tree, starts, aliases, start)
            units.append(
                unit(
                    item.name,
                    class_kind(item, item_aliases, item_assignments),
                    start,
                    end,
                    ordinal,
                )
            )
            unit_by_node[id(item)] = units[-1]
            ordinal += 1
            class_framework = enclosing_class_framework(item, item_aliases, item_assignments)
            for child in item.body:
                if isinstance(child, (ast.FunctionDef, ast.AsyncFunctionDef)):
                    start, end = node_range(starts, child)
                    child_aliases = aliases_at_offset(tree, starts, aliases, start)
                    child_assignments = collect_assignment_roles_until(tree, starts, aliases, start)
                    units.append(
                        unit(
                            child.name,
                            function_kind(
                                child,
                                item.name,
                                child_aliases,
                                child_assignments,
                                class_framework,
                            ),
                            start,
                            end,
                            ordinal,
                        )
                    )
                    unit_by_node[id(child)] = units[-1]
                    ordinal += 1

    for item in tree.body:
        if isinstance(item, (ast.FunctionDef, ast.AsyncFunctionDef)):
            subject_unit_id = unit_id(path, unit_by_node[id(item)])
            item_unit = unit_by_node[id(item)]
            item_aliases = aliases_at_offset(tree, starts, aliases, item_unit["start_byte"])
            item_assignments = collect_assignment_roles_until(
                tree,
                starts,
                aliases,
                item_unit["start_byte"],
            )
            add_unknown_specs_for_unit(
                facts,
                subject_unit_id,
                module_dynamic_unknowns,
                path,
                content_hash_value,
                repository_revision,
                (item_unit["start_byte"], item_unit["end_byte"]),
            )
            collect_decorator_facts(
                item,
                starts,
                path,
                content_hash_value,
                repository_revision,
                subject_unit_id,
                item_aliases,
                item_assignments,
                defined_names,
                facts,
            )
            _fixture_names, has_unknown_fixture_name = pytest_fixture_binding_names(
                item, item_aliases, item_assignments
            )
            if has_unknown_fixture_name:
                start, end = node_range(starts, item)
                add_pytest_fixture_unknown(
                    facts,
                    subject_unit_id,
                    "PytestFixtureInjection",
                    path,
                    content_hash_value,
                    repository_revision,
                    start,
                    end,
                )
            item_kind = unit_by_node[id(item)]["kind"]
            if item_kind == "fastapi_route":
                collect_fastapi_parameter_facts(
                    item,
                    starts,
                    path,
                    content_hash_value,
                    repository_revision,
                    subject_unit_id,
                    item_aliases,
                    facts,
                )
            elif item_kind == "flask_route":
                collect_flask_route_facts(
                    item,
                    starts,
                    path,
                    content_hash_value,
                    repository_revision,
                    subject_unit_id,
                    item_aliases,
                    item_assignments,
                    facts,
                )
            elif item_kind in {"click_command", "typer_command"}:
                collect_cli_command_facts(
                    item,
                    item_kind,
                    starts,
                    path,
                    content_hash_value,
                    repository_revision,
                    subject_unit_id,
                    item_aliases,
                    item_assignments,
                    facts,
                )
            elif item_kind == "celery_task":
                collect_celery_task_facts(
                    item,
                    starts,
                    path,
                    content_hash_value,
                    repository_revision,
                    subject_unit_id,
                    item_aliases,
                    item_assignments,
                    facts,
                )
            (
                initial_dynamic_call_aliases,
                initial_namespace_aliases,
                initial_dynamic_value_aliases,
            ) = collect_dynamic_bindings_until(tree, starts, aliases, item_unit["start_byte"])
            parametrize_names, indirect_parametrize_names = pytest_parametrize_name_sets(
                item, item_aliases, item_assignments
            )
            collect_fixture_facts(
                item,
                starts,
                path,
                content_hash_value,
                repository_revision,
                subject_unit_id,
                fixture_name_counts,
                conftest_fixture_name_counts,
                parametrize_names,
                indirect_parametrize_names,
                item_aliases,
                item_assignments,
                facts,
            )
            collect_call_facts(
                item,
                unit_by_node[id(item)]["kind"],
                starts,
                path,
                content_hash_value,
                repository_revision,
                subject_unit_id,
                item_aliases,
                item_assignments,
                collect_parameter_roles(item, item_aliases),
                defined_names,
                module_index,
                facts,
                initial_dynamic_call_aliases,
                initial_namespace_aliases,
                initial_dynamic_value_aliases,
                custom_query_wrappers=module_custom_query_wrappers,
            )
        elif isinstance(item, ast.ClassDef):
            subject_unit_id = unit_id(path, unit_by_node[id(item)])
            item_unit = unit_by_node[id(item)]
            item_aliases = aliases_at_offset(tree, starts, aliases, item_unit["start_byte"])
            item_assignments = collect_assignment_roles_until(
                tree,
                starts,
                aliases,
                item_unit["start_byte"],
            )
            add_unknown_specs_for_unit(
                facts,
                subject_unit_id,
                module_dynamic_unknowns,
                path,
                content_hash_value,
                repository_revision,
                (item_unit["start_byte"], item_unit["end_byte"]),
            )
            collect_class_base_facts(
                item,
                starts,
                path,
                content_hash_value,
                repository_revision,
                subject_unit_id,
                item_aliases,
                item_assignments,
                facts,
            )
            collect_external_framework_base_unknown_facts(
                item,
                starts,
                path,
                content_hash_value,
                repository_revision,
                subject_unit_id,
                item_aliases,
                item_assignments,
                facts,
            )
            collect_pydantic_model_member_facts(
                item,
                starts,
                path,
                content_hash_value,
                repository_revision,
                subject_unit_id,
                item_aliases,
                facts,
            )
            collect_sqlalchemy_model_field_facts(
                item,
                local_class_names,
                starts,
                path,
                content_hash_value,
                repository_revision,
                subject_unit_id,
                item_aliases,
                facts,
            )
            class_item_kind = item_unit["kind"]
            if class_item_kind == "django_model":
                collect_django_model_facts(
                    item,
                    starts,
                    path,
                    content_hash_value,
                    repository_revision,
                    subject_unit_id,
                    item_aliases,
                    item_assignments,
                    facts,
                )
            elif class_item_kind == "django_test":
                collect_django_test_facts(
                    item,
                    starts,
                    path,
                    content_hash_value,
                    repository_revision,
                    subject_unit_id,
                    facts,
                )
            collect_decorator_facts(
                item,
                starts,
                path,
                content_hash_value,
                repository_revision,
                subject_unit_id,
                item_aliases,
                item_assignments,
                defined_names,
                facts,
            )
            instance_attribute_roles = collect_instance_attribute_roles(item, item_aliases)
            class_custom_query_wrappers = collect_sqlalchemy_custom_query_wrapper_names(
                item,
                item_aliases,
                instance_attribute_roles,
            )
            for child in item.body:
                if not isinstance(child, (ast.FunctionDef, ast.AsyncFunctionDef)):
                    continue
                child_unit_id = unit_id(path, unit_by_node[id(child)])
                child_unit = unit_by_node[id(child)]
                child_aliases = aliases_at_offset(tree, starts, aliases, child_unit["start_byte"])
                child_assignments = collect_assignment_roles_until(
                    tree,
                    starts,
                    aliases,
                    child_unit["start_byte"],
                )
                add_unknown_specs_for_unit(
                    facts,
                    child_unit_id,
                    module_dynamic_unknowns,
                    path,
                    content_hash_value,
                    repository_revision,
                    (child_unit["start_byte"], child_unit["end_byte"]),
                )
                parameter_roles = {
                    **collect_parameter_roles(child, child_aliases),
                    **instance_attribute_roles,
                }
                (
                    initial_dynamic_call_aliases,
                    initial_namespace_aliases,
                    initial_dynamic_value_aliases,
                ) = collect_dynamic_bindings_until(tree, starts, aliases, child_unit["start_byte"])
                collect_decorator_facts(
                    child,
                    starts,
                    path,
                    content_hash_value,
                    repository_revision,
                    child_unit_id,
                    child_aliases,
                    child_assignments,
                    defined_names,
                    facts,
                )
                if child_unit["kind"] == "unittest_test_method":
                    collect_unittest_test_facts(
                        child,
                        item,
                        starts,
                        path,
                        content_hash_value,
                        repository_revision,
                        child_unit_id,
                        child_aliases,
                        child_assignments,
                        facts,
                    )
                collect_call_facts(
                    child,
                    unit_by_node[id(child)]["kind"],
                    starts,
                    path,
                    content_hash_value,
                    repository_revision,
                    child_unit_id,
                    child_aliases,
                    child_assignments,
                    parameter_roles,
                    defined_names,
                    module_index,
                    facts,
                    initial_dynamic_call_aliases,
                    initial_namespace_aliases,
                    initial_dynamic_value_aliases,
                    custom_query_wrappers={
                        *module_custom_query_wrappers,
                        *class_custom_query_wrappers,
                    },
                )

    ordinal = collect_django_url_patterns(
        tree,
        starts,
        path,
        content_hash_value,
        repository_revision,
        aliases,
        module_unit_id,
        units,
        facts,
        ordinal,
    )

    units.sort(key=lambda item: (item["start_byte"], item["end_byte"], item["kind"], item["name"]))
    facts.sort(
        key=lambda item: (
            item["evidence"]["start_byte"],
            item["evidence"]["end_byte"],
            item["fact_kind"],
            item["target"] or "",
            item["subject"],
        )
    )
    return units, diagnostics, facts


def parse_document(payload: dict[str, Any]) -> int:
    if (
        not isinstance(payload, dict)
        or payload.get("protocol_version") != PROTOCOL_VERSION
        or payload.get("contract_revision") != PARSE_DOCUMENT_CONTRACT_REVISION
    ):
        emit_parse_document_contract_mismatch()
        return 0
    required_fields = {
        "protocol_version",
        "contract_revision",
        "mode",
        "path",
        "content_hash",
        "repository_revision",
        "text",
    }
    allowed_fields = {*required_fields, "module_paths", "source_roots", "conftest_files", "module_files"}
    if not isinstance(payload, dict) or not required_fields.issubset(payload) or set(payload) - allowed_fields:
        return 2
    if payload.get("mode") != "parse_document":
        return 2
    if not is_safe_repo_relative_path(payload.get("path")) or not is_strict_content_hash(payload.get("content_hash")):
        return 2
    text = payload.get("text")
    if not isinstance(text, str):
        return 2
    module_paths = safe_path_list(payload.get("module_paths"), require_python=True)
    source_roots = safe_path_list(payload.get("source_roots"), require_python=False)
    conftest_files = safe_conftest_file_records(payload.get("conftest_files"))
    module_files = safe_module_file_records(payload.get("module_files"))
    if module_paths is None or source_roots is None or conftest_files is None or module_files is None:
        return 2
    source_roots = sorted(set([*source_roots, *infer_source_roots(module_paths)]))
    module_index = build_module_index(module_paths, source_roots)
    module_symbols, module_all_names = build_module_symbol_index(module_files, source_roots)
    fixture_index = conftest_fixture_index(conftest_files)
    units, diagnostics, facts = analyze_source(
        payload["path"],
        text,
        payload["content_hash"],
        payload["repository_revision"],
        module_index if module_paths else None,
        source_roots,
        applicable_conftest_fixture_name_counts(payload["path"], fixture_index),
        module_symbols if module_files else None,
        module_all_names if module_files else None,
    )
    message(
        {
            "protocol_version": PROTOCOL_VERSION,
            "contract_revision": PARSE_DOCUMENT_CONTRACT_REVISION,
            "mode": "parse_document",
            "path": payload["path"],
            "units": units,
            "facts": facts,
            "diagnostics": diagnostics,
        }
    )
    return 0


def safe_project_name(value: Any) -> str | None:
    if isinstance(value, str) and re.fullmatch(r"[A-Za-z0-9_.-]{1,128}", value):
        return value
    return None


def config_string_list(value: Any) -> list[str]:
    values = value if isinstance(value, list) else [value]
    result: list[str] = []
    for item in values:
        if isinstance(item, str) and is_safe_repo_relative_path(item):
            result.append(item)
    return result


def is_setup_cfg_path(path: str) -> bool:
    return path == "setup.cfg" or path.endswith("/setup.cfg")


def parse_setup_cfg_text(text: str) -> tuple[dict[str, Any], list[dict[str, str]]]:
    """Parse a `setup.cfg` project config (INI) with the standard-library
    `configparser`. Only sanitized, source-tied context is extracted: the safe
    project name from `[metadata]`, and repo-relative source roots from
    `[tool:pytest]` test paths and `[options.packages.find] where`. This never
    executes setup.py, resolves dependencies, or proves any family claim."""
    unknowns: list[dict[str, str]] = []
    config: dict[str, Any] = {
        "project_name": None,
        "source_roots": [],
        "tool_sections": [],
    }
    parser = configparser.ConfigParser()
    try:
        parser.read_string(text)
    except (configparser.Error, ValueError, UnicodeDecodeError):
        unknowns.append(
            {
                "reason": "MissingProjectConfig",
                "affected_claim": "python_project_config",
            }
        )
        return config, unknowns

    if parser.has_option("metadata", "name"):
        config["project_name"] = safe_project_name(parser.get("metadata", "name", fallback=None))
    roots: set[str] = set()
    if parser.has_section("tool:pytest"):
        config["tool_sections"] = ["pytest"]
        for option in ("testpaths", "pythonpath"):
            if parser.has_option("tool:pytest", option):
                roots.update(config_string_list(parser.get("tool:pytest", option).split()))
    if parser.has_option("options.packages.find", "where"):
        roots.update(config_string_list(parser.get("options.packages.find", "where").split()))
    config["source_roots"] = sorted(roots)
    return config, unknowns


def is_setup_py_path(path: str) -> bool:
    return path == "setup.py" or path.endswith("/setup.py")


SETUPTOOLS_SETUP_TARGET = "setuptools.setup"
SETUPTOOLS_PACKAGE_FINDER_TARGETS = {
    "setuptools.find_packages",
    "setuptools.find_namespace_packages",
}
SETUPTOOLS_TRUSTED_IMPORT_TARGETS = {
    "setuptools",
    SETUPTOOLS_SETUP_TARGET,
    *SETUPTOOLS_PACKAGE_FINDER_TARGETS,
}
SETUPTOOLS_MUTATION_SENSITIVE_ATTRIBUTES = {
    "__dict__",
    "setup",
    "find_packages",
    "find_namespace_packages",
}
SETUPTOOLS_NAMESPACE_MUTATION_METHODS = {
    "__delitem__",
    "__setitem__",
    "clear",
    "pop",
    "popitem",
    "setdefault",
    "update",
}
SETUPTOOLS_RELEVANT_SETUP_KEYWORDS = {"name", "package_dir", "packages"}
AST_MATCH_NAME_NODE_TYPES = tuple(
    node_type
    for node_name in ("MatchAs", "MatchStar")
    if (node_type := getattr(ast, node_name, None)) is not None
)
AST_MATCH_MAPPING_NODE_TYPE = getattr(ast, "MatchMapping", None)


def apply_setup_py_import_bindings(node: ast.AST, bindings: dict[str, str]) -> bool:
    """Apply one direct module-body import to the trusted setup.py bindings.
    Only `import setuptools [as ...]` and explicit supported symbols imported
    from `setuptools` are authoritative; every other import overwrites a
    colliding local name with an untrusted binding. Star imports clear the state
    because they may overwrite any visible name."""

    if isinstance(node, ast.Import):
        for alias in node.names:
            local = alias.asname or alias.name.split(".")[0]
            if alias.name == "setuptools":
                bindings[local] = "setuptools"
            else:
                bindings.pop(local, None)
        return True
    if not isinstance(node, ast.ImportFrom):
        return False
    if any(alias.name == "*" for alias in node.names):
        bindings.clear()
        return True
    for alias in node.names:
        local = alias.asname or alias.name
        target = f"{node.module}.{alias.name}" if not node.level and node.module else None
        if target in SETUPTOOLS_TRUSTED_IMPORT_TARGETS and node.module == "setuptools":
            bindings[local] = target
        else:
            bindings.pop(local, None)
    return True


def setup_py_namespace_target(node: ast.AST) -> tuple[str | None, bool]:
    """Return a lexical module-alias root or a whole-namespace mutation flag."""

    if isinstance(node, ast.Call):
        target = dotted_name(node.func)
        leaf = target.rsplit(".", 1)[-1] if target else None
        if leaf in {"globals", "locals"}:
            return None, True
        if leaf == "vars":
            if not node.args:
                return None, True
            root = dotted_name(node.args[0])
            return (root.split(".", 1)[0], False) if root else (None, True)
    if isinstance(node, ast.Attribute) and node.attr == "__dict__":
        root = dotted_name(node.value)
        return (root.split(".", 1)[0], False) if root else (None, True)
    return None, False


def setup_py_statement_rebindings(node: ast.AST) -> tuple[set[str], set[str], bool]:
    """Collect possible bindings/deletions from one statement in one AST walk.
    Nested control-flow bindings are included so branch-dependent state fails
    closed. A star import or dynamic namespace mutation invalidates all trusted
    bindings. Counting bindings inside nested scopes is intentionally
    conservative: it may abstain but cannot manufacture config facts."""

    names: set[str] = set()
    mutated_roots: set[str] = set()
    invalidate_all = False
    for child in ast.walk(node):
        if isinstance(child, ast.Name) and isinstance(child.ctx, (ast.Store, ast.Del)):
            names.add(child.id)
        elif isinstance(child, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef)):
            names.add(child.name)
        elif isinstance(child, ast.Import):
            names.update(import_local_names(child))
        elif isinstance(child, ast.ImportFrom):
            if any(alias.name == "*" for alias in child.names):
                invalidate_all = True
            names.update(import_local_names(child))
        elif isinstance(child, ast.ExceptHandler) and child.name:
            names.add(child.name)
        elif AST_MATCH_NAME_NODE_TYPES and isinstance(child, AST_MATCH_NAME_NODE_TYPES):
            if child.name:
                names.add(child.name)
        elif AST_MATCH_MAPPING_NODE_TYPE is not None and isinstance(
            child, AST_MATCH_MAPPING_NODE_TYPE
        ):
            if child.rest:
                names.add(child.rest)
        elif isinstance(child, ast.Attribute) and isinstance(
            child.ctx, (ast.Store, ast.Del)
        ):
            target = dotted_name(child)
            if target and child.attr in SETUPTOOLS_MUTATION_SENSITIVE_ATTRIBUTES:
                mutated_roots.add(target.split(".", 1)[0])
        elif (
            isinstance(child, ast.Call)
            and (target := dotted_name(child.func)) is not None
            and target.rsplit(".", 1)[-1] in {"setattr", "delattr"}
            and child.args
        ):
            attribute = (
                literal_string_value(child.args[1]) if len(child.args) > 1 else None
            )
            if attribute is None or attribute in SETUPTOOLS_MUTATION_SENSITIVE_ATTRIBUTES:
                target = dotted_name(child.args[0])
                if target:
                    mutated_roots.add(target.split(".", 1)[0])
                else:
                    invalidate_all = True
        elif (
            isinstance(child, ast.Call)
            and (target := dotted_name(child.func)) is not None
            and target.rsplit(".", 1)[-1] == "exec"
        ):
            invalidate_all = True
        elif (
            isinstance(child, ast.Call)
            and isinstance(child.func, ast.Attribute)
            and child.func.attr in SETUPTOOLS_NAMESPACE_MUTATION_METHODS
        ):
            root, mutates_all = setup_py_namespace_target(child.func.value)
            if mutates_all:
                invalidate_all = True
            elif root:
                mutated_roots.add(root)
        elif (
            isinstance(child, ast.Subscript)
            and isinstance(child.ctx, (ast.Store, ast.Del))
        ):
            root, mutates_all = setup_py_namespace_target(child.value)
            if mutates_all:
                invalidate_all = True
            elif root:
                attribute = literal_string_value(child.slice)
                if attribute is None or attribute in SETUPTOOLS_MUTATION_SENSITIVE_ATTRIBUTES:
                    mutated_roots.add(root)
    return names, mutated_roots, invalidate_all


def setup_py_call_is_bound_to(
    bindings: dict[str, str],
    statement_rebindings: set[str],
    node: ast.Call,
    expected_target: str,
) -> bool:
    raw_target = dotted_name(node.func)
    if raw_target is None:
        return False
    parts = raw_target.split(".")
    root = parts[0]
    if root in statement_rebindings or root not in bindings:
        return False
    if len(parts) > 1 and parts[-1] != expected_target.rsplit(".", 1)[-1]:
        return False
    return canonical_name(raw_target, bindings, {}) == expected_target


def scan_authoritative_setup_py_calls(
    tree: ast.Module,
) -> tuple[list[tuple[ast.Call, dict[str, str], set[str]]], int, bool]:
    """Scan module statements once in source order and stop at two trusted setup
    calls, which is already a conflict. The count is returned for deterministic
    linear-scan regression tests; the final flag records a definite direct
    top-level termination before any setup call. Neither is serialized."""

    bindings: dict[str, str] = {}
    setup_calls: list[tuple[ast.Call, dict[str, str], set[str]]] = []
    scanned_statements = 0
    for statement in tree.body:
        scanned_statements += 1
        if isinstance(statement, ast.Raise):
            if not setup_calls:
                return [], scanned_statements, True
            return setup_calls, scanned_statements, False
        if apply_setup_py_import_bindings(statement, bindings):
            continue
        rebound_names, mutated_roots, invalidate_all = setup_py_statement_rebindings(
            statement
        )
        invalidated_roots = rebound_names | mutated_roots
        if isinstance(statement, ast.Expr) and isinstance(statement.value, ast.Call):
            node = statement.value
            if setup_py_call_is_bound_to(
                bindings,
                invalidated_roots,
                node,
                SETUPTOOLS_SETUP_TARGET,
            ):
                setup_calls.append((node, dict(bindings), invalidated_roots))
                if len(setup_calls) > 1:
                    break
        if invalidate_all:
            bindings.clear()
        else:
            for name in invalidated_roots:
                bindings.pop(name, None)
    return setup_calls, scanned_statements, False


def literal_package_finder_root(node: ast.Call) -> tuple[str | None, bool]:
    """Return a literal finder root only for an unambiguous official call shape."""

    if len(node.args) > 1 or any(keyword.arg is None for keyword in node.keywords):
        return None, False
    where_nodes: list[ast.AST] = []
    if node.args:
        where_nodes.append(node.args[0])
    for keyword in node.keywords:
        if keyword.arg == "where":
            where_nodes.append(keyword.value)
    if len(where_nodes) > 1:
        return None, False
    if not where_nodes:
        return None, True
    where = literal_string_value(where_nodes[0])
    return (where, True) if where is not None else (None, False)


def literal_package_dir_roots(node: ast.AST) -> list[str] | None:
    """Return roots only for a complete unique string-to-string dict literal."""

    if not isinstance(node, ast.Dict):
        return None
    roots: list[str] = []
    seen_keys: set[str] = set()
    for key_node, value_node in zip(node.keys, node.values):
        key = literal_string_value(key_node)
        value = literal_string_value(value_node)
        if key is None or value is None or key in seen_keys:
            return None
        seen_keys.add(key)
        roots.append(value)
    return roots


def setup_py_relevant_keywords(node: ast.Call) -> tuple[dict[str, ast.AST], bool]:
    """Return unique relevant setup keywords when the call shape is complete."""

    if node.args or any(keyword.arg is None for keyword in node.keywords):
        return {}, False
    values: dict[str, ast.AST] = {}
    for keyword in node.keywords:
        if keyword.arg not in SETUPTOOLS_RELEVANT_SETUP_KEYWORDS:
            continue
        if keyword.arg in values:
            return {}, False
        values[keyword.arg] = keyword.value
    return values, True


def static_packages_root(
    node: ast.AST,
    bindings: dict[str, str],
    statement_rebindings: set[str],
) -> tuple[str | None, bool]:
    """Classify a static packages value and extract an explicit finder root."""

    if isinstance(node, ast.Call):
        if not any(
            setup_py_call_is_bound_to(
                bindings,
                statement_rebindings,
                node,
                target,
            )
            for target in SETUPTOOLS_PACKAGE_FINDER_TARGETS
        ):
            return None, False
        return literal_package_finder_root(node)
    if isinstance(node, ast.Constant) and node.value is None:
        return None, True
    if isinstance(node, (ast.List, ast.Tuple, ast.Set)) and all(
        literal_string_value(element) is not None for element in node.elts
    ):
        return None, True
    return None, False


def missing_python_project_config_unknown() -> dict[str, str]:
    return {
        "reason": "MissingProjectConfig",
        "affected_claim": "python_project_config",
    }


def parse_setup_py_text(text: str) -> tuple[dict[str, Any], list[dict[str, str]]]:
    """Extract sanitized source roots from a `setup.py` using the standard-library
    `ast` module WITHOUT executing it. Only literal source-root evidence is read:
    the safe project name from a unique literal `name=` keyword, every value of
    a complete unique string-to-string `package_dir` dict, and at most one
    unambiguous literal `where=` (or sole positional value) of
    `find_packages(...)` / `find_namespace_packages(...)` used directly as the
    `packages=` value of a direct, unconditional zero-positional module-body
    `setup(...)` call with no keyword unpacking. Calls are accepted only when one
    source-order binding scan,
    reusing the worker's canonical-name/import-local helpers, lexically resolves
    the callee through `setuptools` and observes no relevant name, attribute, or
    namespace mutation. Same-leaf local/helper calls, conditional calls, and
    shadowed/deleted/mutated bindings abstain. A recognized unique setup call
    with a dynamic, incomplete, duplicate, unpacked, unreachable, or otherwise
    overridable relevant field emits `MissingProjectConfig` and contributes no
    roots from that field; `setup()` remains a complete empty config. Exactly one
    authoritative setup call is required; multiple calls produce
    `ConflictingFacts`, while syntax that does not parse yields
    `MissingProjectConfig`. This never executes `setup.py`, runs a finder,
    resolves dependencies, or proves any family claim."""
    unknowns: list[dict[str, str]] = []
    config: dict[str, Any] = {
        "project_name": None,
        "source_roots": [],
        "tool_sections": [],
    }
    try:
        tree = ast.parse(text)
    except (SyntaxError, ValueError):
        unknowns.append(
            {
                "reason": "MissingProjectConfig",
                "affected_claim": "python_project_config",
            }
        )
        return config, unknowns

    setup_calls, _scanned_statements, terminated_before_setup = (
        scan_authoritative_setup_py_calls(tree)
    )
    if terminated_before_setup:
        unknowns.append(missing_python_project_config_unknown())
        return config, unknowns
    if len(setup_calls) > 1:
        unknowns.append(
            {
                "reason": "ConflictingFacts",
                "affected_claim": "python_project_config",
            }
        )
        return config, unknowns
    if not setup_calls:
        return config, unknowns

    name: str | None = None
    roots: set[str] = set()
    setup_call, setup_bindings, statement_rebindings = setup_calls[0]
    setup_keywords, complete_setup_shape = setup_py_relevant_keywords(setup_call)
    if not complete_setup_shape:
        unknowns.append(missing_python_project_config_unknown())
        return config, unknowns

    incomplete_field = False
    name_node = setup_keywords.get("name")
    if name_node is not None:
        literal_name = literal_string_value(name_node)
        if literal_name is None or safe_project_name(literal_name) is None:
            incomplete_field = True
        else:
            name = literal_name

    package_dir_node = setup_keywords.get("package_dir")
    if package_dir_node is not None:
        package_dir_roots = literal_package_dir_roots(package_dir_node)
        if package_dir_roots is None:
            incomplete_field = True
        else:
            roots.update(package_dir_roots)

    packages_node = setup_keywords.get("packages")
    if packages_node is not None:
        packages_root, complete_packages = static_packages_root(
            packages_node,
            setup_bindings,
            statement_rebindings,
        )
        if not complete_packages:
            incomplete_field = True
        elif packages_root is not None:
            roots.add(packages_root)

    if incomplete_field:
        unknowns.append(missing_python_project_config_unknown())
    config["project_name"] = safe_project_name(name)
    config["source_roots"] = sorted(config_string_list(sorted(roots)))
    return config, unknowns


def parse_project_config_text(
    text: str, path: str = "pyproject.toml"
) -> tuple[dict[str, Any], list[dict[str, str]]]:
    if is_setup_cfg_path(path):
        return parse_setup_cfg_text(text)
    if is_setup_py_path(path):
        return parse_setup_py_text(text)
    unknowns: list[dict[str, str]] = []
    config = {
        "project_name": None,
        "source_roots": [],
        "tool_sections": [],
    }
    if tomllib is None:
        unknowns.append(
            {
                "reason": "MissingDependency",
                "affected_claim": "python_project_config",
            }
        )
        return config, unknowns

    try:
        data = tomllib.loads(text)
    except tomllib.TOMLDecodeError:
        unknowns.append(
            {
                "reason": "MissingProjectConfig",
                "affected_claim": "python_project_config",
            }
        )
        return config, unknowns

    project = data.get("project") if isinstance(data, dict) else None
    if isinstance(project, dict):
        config["project_name"] = safe_project_name(project.get("name"))
    tool = data.get("tool") if isinstance(data, dict) else None
    if isinstance(tool, dict):
        config["tool_sections"] = sorted(
            section
            for section in ["pytest", "pyrefly", "pyright"]
            if isinstance(tool.get(section), dict)
        )
        roots: set[str] = set()
        pytest_config = tool.get("pytest")
        if isinstance(pytest_config, dict):
            ini_options = pytest_config.get("ini_options")
            if isinstance(ini_options, dict):
                roots.update(config_string_list(ini_options.get("testpaths")))
                roots.update(config_string_list(ini_options.get("pythonpath")))
        pyright_config = tool.get("pyright")
        if isinstance(pyright_config, dict):
            roots.update(config_string_list(pyright_config.get("include")))
            roots.update(config_string_list(pyright_config.get("extraPaths")))
        config["source_roots"] = sorted(roots)
    return config, unknowns


def parse_project_config(payload: dict[str, Any]) -> int:
    if set(payload) != {
        "protocol_version",
        "mode",
        "path",
        "content_hash",
        "repository_revision",
        "text",
    }:
        return 2
    if payload.get("protocol_version") != PROTOCOL_VERSION or payload.get("mode") != "parse_project_config":
        return 2
    if not is_safe_repo_relative_path(payload.get("path")) or not is_strict_content_hash(payload.get("content_hash")):
        return 2
    text = payload.get("text")
    if not isinstance(text, str) or len(text.encode("utf-8")) > MAX_CONFIG_TEXT_BYTES:
        return 2
    config, unknowns = parse_project_config_text(text, payload["path"])
    message(
        {
            "protocol_version": PROTOCOL_VERSION,
            "mode": "parse_project_config",
            "path": payload["path"],
            "config": config,
            "unknowns": unknowns,
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


def read_source(path: Path) -> tuple[str, str] | None:
    try:
        with path.open("rb") as source_file:
            data = source_file.read(MAX_SOURCE_BYTES + 1)
    except OSError:
        return None
    if len(data) > MAX_SOURCE_BYTES:
        return None
    try:
        text = data.decode("utf-8")
    except UnicodeDecodeError:
        return None
    return text, f"sha256:{hashlib.sha256(data).hexdigest()}"


def project_source_roots(project_root: Path) -> list[str]:
    config_path = resolve_under_root(project_root, "pyproject.toml")
    if config_path is None:
        return []
    source_result = read_source(config_path)
    if source_result is None:
        return []
    text, _hash = source_result
    config, unknowns = parse_project_config_text(text)
    if unknowns:
        return []
    roots = config.get("source_roots")
    if not isinstance(roots, list):
        return []
    return sorted(root for root in roots if isinstance(root, str) and is_safe_repo_relative_path(root))


def pytest_fixture_name_counts(source: str) -> dict[str, int]:
    try:
        tree = ast.parse(source)
    except SyntaxError:
        return {}
    starts = byte_line_starts(source)
    aliases, _facts = collect_import_aliases(
        tree,
        starts,
        "conftest.py",
        "sha256:" + "0" * 64,
        "UNKNOWN",
        "unit:conftest.py#module:module:0-0:0",
    )
    assignments = collect_assignment_roles(tree, aliases)
    return pytest_fixture_name_counts_from_tree(tree, aliases, assignments)


def pytest_fixture_names(source: str) -> set[str]:
    return set(pytest_fixture_name_counts(source))


def conftest_fixture_index(file_records: list[tuple[str, str, str]]) -> dict[str, dict[str, int]]:
    result: dict[str, dict[str, int]] = {}
    for relative_path, source, _file_hash in file_records:
        if relative_path == "conftest.py" or relative_path.endswith("/conftest.py"):
            counts = pytest_fixture_name_counts(source)
            if counts:
                result[relative_path] = counts
    return result


def applicable_conftest_fixture_name_counts(
    path: str, fixture_index: dict[str, dict[str, int]]
) -> dict[str, int]:
    counts: dict[str, int] = {}
    current_dir = parent_path(path)
    while True:
        conftest_path = f"{current_dir}/conftest.py" if current_dir else "conftest.py"
        for name, count in fixture_index.get(conftest_path, {}).items():
            counts[name] = counts.get(name, 0) + count
        if not current_dir:
            break
        current_dir = parent_path(current_dir)
    return counts


def safe_conftest_file_records(value: Any) -> list[tuple[str, str, str]] | None:
    if value is None:
        return []
    if not isinstance(value, list):
        return None
    records: list[tuple[str, str, str]] = []
    seen: set[str] = set()
    for item in value:
        if not isinstance(item, dict) or set(item) != {"path", "text"}:
            return None
        path = item.get("path")
        text = item.get("text")
        if (
            not is_safe_repo_relative_path(path)
            or not isinstance(text, str)
            or path in seen
            or not (path == "conftest.py" or path.endswith("/conftest.py"))
        ):
            return None
        seen.add(path)
        records.append((path, text, ""))
    return records


def safe_module_file_records(value: Any) -> list[tuple[str, str, str]] | None:
    if value is None:
        return []
    if not isinstance(value, list):
        return None
    records: list[tuple[str, str, str]] = []
    seen: set[str] = set()
    for item in value:
        if not isinstance(item, dict) or set(item) != {"path", "text"}:
            return None
        path = item.get("path")
        text = item.get("text")
        if (
            not is_safe_repo_relative_path(path)
            or not isinstance(text, str)
            or path in seen
            or not path.endswith(".py")
            or len(text.encode("utf-8")) > MAX_SOURCE_BYTES
        ):
            return None
        seen.add(path)
        records.append((path, text, ""))
    records.sort(key=lambda record: record[0])
    return records


def emit_fact_message(request_id: str, fact_payload: dict[str, Any]) -> None:
    message(
        {
            "protocol_version": PROTOCOL_VERSION,
            "message_type": "fact",
            "request_id": request_id,
            **fact_payload,
        }
    )


def emit_framework_role_fact(
    request_id: str,
    relative_path: str,
    file_hash: str,
    unit_data: dict[str, Any],
) -> None:
    role_by_kind = {
        "fastapi_route": "framework:fastapi.route",
        "pytest_test": "framework:pytest.test",
        "pytest_fixture": "framework:pytest.fixture",
        "pydantic_model": "framework:pydantic.model",
        "sqlalchemy_model": "framework:sqlalchemy.model",
        "sqlalchemy_repository_method": "framework:sqlalchemy.repository_method",
        "django_model": "framework:django.model",
        "django_url_pattern": "framework:django.url_pattern",
        "django_test": "framework:django.test",
        "flask_route": "framework:flask.route",
        "unittest_test_method": "framework:unittest.test",
        "click_command": "framework:click.command",
        "typer_command": "framework:typer.command",
        "celery_task": "framework:celery.task",
    }
    role = role_by_kind.get(unit_data["kind"])
    if role is None:
        return
    subject_unit_id = unit_id(relative_path, unit_data)
    emit_fact_message(
        request_id,
        fact(
            kind="FRAMEWORK_ROLE",
            subject=subject_unit_id,
            target=role,
            certainty="FRAMEWORK_HEURISTIC",
            path=relative_path,
            content_hash_value=file_hash,
            repository_revision="UNKNOWN",
            subject_unit_id=subject_unit_id,
            start=unit_data["start_byte"],
            end=unit_data["end_byte"],
            note=f"CPython ast recognized {role}",
            assumptions=["binding unresolved without provider"],
        ),
    )


def analyze_project(payload: dict[str, Any]) -> int:
    request_id = payload.get("request_id") if isinstance(payload, dict) else DEFAULT_REQUEST_ID
    if not isinstance(request_id, str) or not request_id.strip():
        request_id = DEFAULT_REQUEST_ID
    if not validate_request(payload):
        emit_worker_error(request_id, "SEMANTIC_PROTOCOL_VIOLATION", "semantic worker request is invalid")
        return 0
    project_root = Path(payload["project_root"])
    file_records: list[tuple[str, str, str]] = []
    total_source_bytes = 0
    for relative_path in sorted(payload["changed_files"]):
        if not relative_path.endswith(".py"):
            continue
        file_path = resolve_under_root(project_root, relative_path)
        if file_path is None:
            continue
        source_result = read_source(file_path)
        if source_result is None:
            continue
        source, file_hash = source_result
        total_source_bytes += len(source.encode("utf-8"))
        if total_source_bytes > MAX_TOTAL_SOURCE_BYTES:
            emit_worker_error(
                request_id,
                "SEMANTIC_PROTOCOL_VIOLATION",
                "semantic worker request exceeds the aggregate source-size budget",
            )
            return 0
        file_records.append((relative_path, source, file_hash))
    source_roots = project_source_roots(project_root)
    module_index = build_module_index(
        [relative_path for relative_path, _source, _file_hash in file_records],
        source_roots,
    )
    module_symbols, module_all_names = build_module_symbol_index(file_records, source_roots)
    fixture_index = conftest_fixture_index(file_records)
    for relative_path, source, file_hash in file_records:
        units, _diagnostics, facts = analyze_source(
            relative_path,
            source,
            file_hash,
            "UNKNOWN",
            module_index,
            source_roots,
            applicable_conftest_fixture_name_counts(relative_path, fixture_index),
            module_symbols,
            module_all_names,
        )
        for fact_payload in facts:
            emit_fact_message(request_id, fact_payload)
        for unit_data in units:
            emit_framework_role_fact(request_id, relative_path, file_hash, unit_data)
    message({"protocol_version": PROTOCOL_VERSION, "message_type": "end_of_stream", "request_id": request_id})
    return 0


def dispatch(payload: Any) -> int:
    if isinstance(payload, dict) and payload.get("mode") == "parse_document":
        return parse_document(payload)
    if isinstance(payload, dict) and payload.get("mode") == "parse_project_config":
        return parse_project_config(payload)
    return analyze_project(payload)


def main() -> int:
    try:
        payload = json.loads(read_stdin())
    except Exception:
        emit_worker_error(DEFAULT_REQUEST_ID, "SEMANTIC_PROTOCOL_VIOLATION", "semantic worker request is invalid")
        return 0
    try:
        return dispatch(payload)
    except Exception:
        # Any unexpected failure (e.g. RecursionError, MemoryError) must still
        # produce a typed worker_error + end_of_stream so the host observes a
        # clean UNKNOWN instead of a truncated NDJSON stream and a nonzero exit.
        request_id = payload.get("request_id") if isinstance(payload, dict) else DEFAULT_REQUEST_ID
        if not isinstance(request_id, str) or not request_id.strip():
            request_id = DEFAULT_REQUEST_ID
        emit_worker_error(request_id, "SEMANTIC_WORKER_FAILURE", "semantic worker failed to complete analysis")
        return 0


if __name__ == "__main__":
    raise SystemExit(main())
