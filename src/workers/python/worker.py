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
import symtable
from pathlib import Path
from typing import Any

try:
    import tomllib
except ModuleNotFoundError:  # Python < 3.11.
    tomllib = None

PROTOCOL_VERSION = 1
DEFAULT_REQUEST_ID = "repogrammar-python-semantic-worker"
MAX_STDIN_BYTES = 1_048_576
MAX_PROJECT_ROOT_CHARS = 4096
MAX_CHANGED_FILES = 10_000
MAX_PATH_CHARS = 4096
MAX_SOURCE_BYTES = 1_048_576
MAX_FACTS_PER_FILE = 2_000
MAX_FACT_TARGET_CHARS = 512
MAX_CONFIG_TEXT_BYTES = 1_048_576
ROUTE_METHODS = {"delete", "get", "head", "options", "patch", "post", "put"}
SQLALCHEMY_SESSION_METHODS = {"commit", "execute", "rollback"}
SQLALCHEMY_SESSION_TYPES = {
    "sqlalchemy.orm.Session",
    "sqlalchemy.ext.asyncio.AsyncSession",
}


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
        and not value.startswith("/")
        and "\\" not in value
        and not has_windows_drive_prefix(value)
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


def unit_id(path: str, unit_data: dict[str, Any]) -> str:
    return (
        f"unit:{path}#{unit_data['kind']}:{slug(unit_data['name'])}:"
        f"{unit_data['start_byte']}-{unit_data['end_byte']}:{unit_data['ordinal']}"
    )


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


def has_fastapi_route_decorator(node: ast.AST) -> bool:
    for name in decorator_names(node):
        parts = name.split(".")
        if len(parts) >= 2 and parts[-1] in ROUTE_METHODS:
            return True
    return False


def has_pytest_fixture_decorator(node: ast.AST) -> bool:
    return any(name in {"fixture", "pytest.fixture"} for name in decorator_names(node))


def pytest_fixture_names_from_tree(tree: ast.Module) -> set[str]:
    return {
        item.name
        for item in tree.body
        if isinstance(item, (ast.FunctionDef, ast.AsyncFunctionDef)) and has_pytest_fixture_decorator(item)
    }


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


def canonical_name(name: str, aliases: dict[str, str], assignments: dict[str, str]) -> str:
    parts = name.split(".")
    if not parts:
        return name
    if parts[0] in assignments:
        return ".".join([assignments[parts[0]], *parts[1:]])
    if parts[0] in aliases:
        return ".".join([aliases[parts[0]], *parts[1:]])
    return name


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
) -> tuple[dict[str, str], list[dict[str, Any]]]:
    aliases: dict[str, str] = {}
    facts: list[dict[str, Any]] = []
    imports = [node for node in ast.walk(tree) if isinstance(node, (ast.Import, ast.ImportFrom))]
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
                if repo_resolution == "missing" and repo_local_prefix_exists(target, module_index):
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


def assignment_role(value: ast.AST, aliases: dict[str, str], assignments: dict[str, str]) -> str | None:
    if isinstance(value, ast.Call):
        call_name = dotted_name(value.func)
        if not call_name:
            return None
        canonical = canonical_name(call_name, aliases, {})
        if canonical in {"fastapi.APIRouter", "fastapi.FastAPI"}:
            return canonical
        return None
    if isinstance(value, ast.Name):
        return assignments.get(value.id)
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
    facts: list[dict[str, Any]],
) -> None:
    for decorator in getattr(node, "decorator_list", []):
        name = dotted_name(decorator)
        if not name:
            continue
        start, end = node_range(starts, decorator)
        add_fact(
            facts,
            structural_fact(
                kind="SYMBOL",
                subject_unit_id=subject_unit_id,
                target=canonical_name(name, aliases, assignments),
                path=path,
                content_hash_value=content_hash_value,
                repository_revision=repository_revision,
                start=start,
                end=end,
                anchor_kind="decorator_binding",
            ),
        )


def collect_class_base_facts(
    node: ast.ClassDef,
    starts: list[int],
    path: str,
    content_hash_value: str,
    repository_revision: str,
    subject_unit_id: str,
    aliases: dict[str, str],
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
                target=canonical_name(name, aliases, {}),
                path=path,
                content_hash_value=content_hash_value,
                repository_revision=repository_revision,
                start=start,
                end=end,
                anchor_kind="class_base",
            ),
        )


def collect_sqlalchemy_model_field_facts(
    node: ast.ClassDef,
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
            if canonical_value == "sqlalchemy.orm.mapped_column":
                start, end = node_range(starts, value if isinstance(value, ast.AST) else item)
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
                        anchor_kind="sqlalchemy_mapped_column",
                    ),
                )


def is_dynamic_call(node: ast.Call) -> bool:
    if isinstance(node.func, ast.Call) and dotted_name(node.func) == "getattr":
        return True
    if isinstance(node.func, ast.Subscript) and dotted_name(node.func) == "globals":
        return True
    return False


def collect_call_facts(
    node: ast.AST,
    starts: list[int],
    path: str,
    content_hash_value: str,
    repository_revision: str,
    subject_unit_id: str,
    aliases: dict[str, str],
    assignments: dict[str, str],
    parameter_roles: dict[str, str] | None,
    facts: list[dict[str, Any]],
) -> None:
    parameter_roles = parameter_roles or {}
    calls = [child for child in ast.walk(node) if isinstance(child, ast.Call)]
    calls.sort(key=lambda child: node_range(starts, child))
    for call in calls:
        start, end = node_range(starts, call)
        name = dotted_name(call.func)
        canonical = canonical_name(name, aliases, assignments) if name else None
        if canonical:
            parts = canonical.split(".")
            if (
                len(parts) == 2
                and parts[0] in parameter_roles
                and parts[1] in SQLALCHEMY_SESSION_METHODS
            ):
                canonical = f"{parameter_roles[parts[0]]}.{parts[1]}"
        if canonical in {"sys.path.append", "sys.path.insert"}:
            add_fact(
                facts,
                unknown_fact(
                    subject_unit_id=subject_unit_id,
                    reason_code="RuntimeDependencyInjection",
                    affected_claim="python_import_resolution",
                    path=path,
                    content_hash_value=content_hash_value,
                    repository_revision=repository_revision,
                    start=start,
                    end=end,
                ),
            )
            continue
        if canonical == "importlib.import_module":
            first_arg = call.args[0] if call.args else None
            if (
                isinstance(first_arg, ast.Constant)
                and isinstance(first_arg.value, str)
                and is_safe_fact_target(first_arg.value)
            ):
                add_fact(
                    facts,
                    structural_fact(
                        kind="RESOLVED_IMPORT",
                        subject_unit_id=subject_unit_id,
                        target=first_arg.value,
                        path=path,
                        content_hash_value=content_hash_value,
                        repository_revision=repository_revision,
                        start=start,
                        end=end,
                        anchor_kind="dynamic_import_literal",
                    ),
                )
            else:
                add_fact(
                    facts,
                    unknown_fact(
                        subject_unit_id=subject_unit_id,
                        reason_code="DynamicImport",
                        affected_claim="python_import_resolution",
                        path=path,
                        content_hash_value=content_hash_value,
                        repository_revision=repository_revision,
                        start=start,
                        end=end,
                    ),
                )
            continue
        if is_dynamic_call(call):
            add_fact(
                facts,
                unknown_fact(
                    subject_unit_id=subject_unit_id,
                    reason_code="FrameworkMagic",
                    affected_claim="python_call_target",
                    path=path,
                    content_hash_value=content_hash_value,
                    repository_revision=repository_revision,
                    start=start,
                    end=end,
                ),
            )
            continue
        if canonical:
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
                    anchor_kind="call_target",
                ),
            )


def collect_fixture_facts(
    node: ast.FunctionDef | ast.AsyncFunctionDef,
    starts: list[int],
    path: str,
    content_hash_value: str,
    repository_revision: str,
    subject_unit_id: str,
    fixture_names: set[str],
    conftest_fixture_names: set[str] | None,
    facts: list[dict[str, Any]],
) -> None:
    if not node.name.startswith("test_"):
        return
    start, end = node_range(starts, node)
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
    conftest_fixture_names = conftest_fixture_names or set()
    for arg in node.args.args:
        if arg.arg == "self":
            continue
        start, end = node_range(starts, arg)
        if arg.arg in fixture_names:
            add_fact(
                facts,
                structural_fact(
                    kind="SYMBOL",
                    subject_unit_id=subject_unit_id,
                    target=f"pytest.fixture.{arg.arg}",
                    path=path,
                    content_hash_value=content_hash_value,
                    repository_revision=repository_revision,
                    start=start,
                    end=end,
                    anchor_kind="pytest_fixture_edge",
                ),
            )
        elif arg.arg in conftest_fixture_names:
            add_fact(
                facts,
                structural_fact(
                    kind="SYMBOL",
                    subject_unit_id=subject_unit_id,
                    target=f"pytest.fixture.{arg.arg}",
                    path=path,
                    content_hash_value=content_hash_value,
                    repository_revision=repository_revision,
                    start=start,
                    end=end,
                    anchor_kind="pytest_conftest_fixture_edge",
                ),
            )
        else:
            add_fact(
                facts,
                unknown_fact(
                    subject_unit_id=subject_unit_id,
                    reason_code="PytestFixtureInjection",
                    affected_claim="pytest_fixture_binding",
                    path=path,
                    content_hash_value=content_hash_value,
                    repository_revision=repository_revision,
                    start=start,
                    end=end,
                ),
            )


def analyze_source(
    path: str,
    source: str,
    content_hash_value: str,
    repository_revision: str,
    module_index: dict[str, list[str]] | None = None,
    source_roots: list[str] | None = None,
    conftest_fixture_names: set[str] | None = None,
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
    )
    for item in import_facts:
        add_fact(facts, item)
    collect_module_identity_and_scope_facts(
        tree,
        path,
        source,
        content_hash_value,
        repository_revision,
        module_unit_id,
        facts,
    )
    assignments = collect_assignment_roles(tree, aliases)
    fixture_names = pytest_fixture_names_from_tree(tree)

    for item in tree.body:
        if isinstance(item, (ast.FunctionDef, ast.AsyncFunctionDef)):
            start, end = node_range(starts, item)
            units.append(unit(item.name, function_kind(item, None), start, end, ordinal))
            unit_by_node[id(item)] = units[-1]
            ordinal += 1
        elif isinstance(item, ast.ClassDef):
            start, end = node_range(starts, item)
            units.append(unit(item.name, class_kind(item), start, end, ordinal))
            unit_by_node[id(item)] = units[-1]
            ordinal += 1
            for child in item.body:
                if isinstance(child, (ast.FunctionDef, ast.AsyncFunctionDef)):
                    start, end = node_range(starts, child)
                    units.append(unit(child.name, function_kind(child, item.name), start, end, ordinal))
                    unit_by_node[id(child)] = units[-1]
                    ordinal += 1

    for item in tree.body:
        if isinstance(item, (ast.FunctionDef, ast.AsyncFunctionDef)):
            subject_unit_id = unit_id(path, unit_by_node[id(item)])
            collect_decorator_facts(
                item,
                starts,
                path,
                content_hash_value,
                repository_revision,
                subject_unit_id,
                aliases,
                assignments,
                facts,
            )
            collect_fixture_facts(
                item,
                starts,
                path,
                content_hash_value,
                repository_revision,
                subject_unit_id,
                fixture_names,
                conftest_fixture_names,
                facts,
            )
            collect_call_facts(
                item,
                starts,
                path,
                content_hash_value,
                repository_revision,
                subject_unit_id,
                aliases,
                assignments,
                collect_parameter_roles(item, aliases),
                facts,
            )
        elif isinstance(item, ast.ClassDef):
            subject_unit_id = unit_id(path, unit_by_node[id(item)])
            collect_class_base_facts(
                item,
                starts,
                path,
                content_hash_value,
                repository_revision,
                subject_unit_id,
                aliases,
                facts,
            )
            collect_sqlalchemy_model_field_facts(
                item,
                starts,
                path,
                content_hash_value,
                repository_revision,
                subject_unit_id,
                aliases,
                facts,
            )
            collect_decorator_facts(
                item,
                starts,
                path,
                content_hash_value,
                repository_revision,
                subject_unit_id,
                aliases,
                assignments,
                facts,
            )
            for child in item.body:
                if not isinstance(child, (ast.FunctionDef, ast.AsyncFunctionDef)):
                    continue
                child_unit_id = unit_id(path, unit_by_node[id(child)])
                collect_decorator_facts(
                    child,
                    starts,
                    path,
                    content_hash_value,
                    repository_revision,
                    child_unit_id,
                    aliases,
                    assignments,
                    facts,
                )
                collect_call_facts(
                    child,
                    starts,
                    path,
                    content_hash_value,
                    repository_revision,
                    child_unit_id,
                    aliases,
                    assignments,
                    collect_parameter_roles(child, aliases),
                    facts,
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
    required_fields = {
        "protocol_version",
        "mode",
        "path",
        "content_hash",
        "repository_revision",
        "text",
    }
    allowed_fields = {*required_fields, "module_paths", "source_roots", "conftest_files"}
    if not isinstance(payload, dict) or not required_fields.issubset(payload) or set(payload) - allowed_fields:
        return 2
    if payload.get("protocol_version") != PROTOCOL_VERSION or payload.get("mode") != "parse_document":
        return 2
    if not is_safe_repo_relative_path(payload.get("path")) or not is_strict_content_hash(payload.get("content_hash")):
        return 2
    text = payload.get("text")
    if not isinstance(text, str):
        return 2
    module_paths = safe_path_list(payload.get("module_paths"), require_python=True)
    source_roots = safe_path_list(payload.get("source_roots"), require_python=False)
    conftest_files = safe_conftest_file_records(payload.get("conftest_files"))
    if module_paths is None or source_roots is None or conftest_files is None:
        return 2
    source_roots = sorted(set([*source_roots, *infer_source_roots(module_paths)]))
    module_index = build_module_index(module_paths, source_roots)
    fixture_index = conftest_fixture_index(conftest_files)
    units, diagnostics, facts = analyze_source(
        payload["path"],
        text,
        payload["content_hash"],
        payload["repository_revision"],
        module_index if module_paths else None,
        source_roots,
        applicable_conftest_fixture_names(payload["path"], fixture_index),
    )
    message(
        {
            "protocol_version": PROTOCOL_VERSION,
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


def parse_project_config_text(text: str) -> tuple[dict[str, Any], list[dict[str, str]]]:
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
    config, unknowns = parse_project_config_text(text)
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


def pytest_fixture_names(source: str) -> set[str]:
    try:
        tree = ast.parse(source)
    except SyntaxError:
        return set()
    return pytest_fixture_names_from_tree(tree)


def conftest_fixture_index(file_records: list[tuple[str, str, str]]) -> dict[str, set[str]]:
    result: dict[str, set[str]] = {}
    for relative_path, source, _file_hash in file_records:
        if relative_path == "conftest.py" or relative_path.endswith("/conftest.py"):
            names = pytest_fixture_names(source)
            if names:
                result[relative_path] = names
    return result


def applicable_conftest_fixture_names(path: str, fixture_index: dict[str, set[str]]) -> set[str]:
    names: set[str] = set()
    current_dir = parent_path(path)
    while True:
        conftest_path = f"{current_dir}/conftest.py" if current_dir else "conftest.py"
        names.update(fixture_index.get(conftest_path, set()))
        if not current_dir:
            break
        current_dir = parent_path(current_dir)
    return names


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
        file_records.append((relative_path, source, file_hash))
    source_roots = project_source_roots(project_root)
    module_index = build_module_index(
        [relative_path for relative_path, _source, _file_hash in file_records],
        source_roots,
    )
    fixture_index = conftest_fixture_index(file_records)
    for relative_path, source, file_hash in file_records:
        units, _diagnostics, facts = analyze_source(
            relative_path,
            source,
            file_hash,
            "UNKNOWN",
            module_index,
            source_roots,
            applicable_conftest_fixture_names(relative_path, fixture_index),
        )
        for fact_payload in facts:
            emit_fact_message(request_id, fact_payload)
        for unit_data in units:
            emit_framework_role_fact(request_id, relative_path, file_hash, unit_data)
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
    if isinstance(payload, dict) and payload.get("mode") == "parse_project_config":
        return parse_project_config(payload)
    return analyze_project(payload)


if __name__ == "__main__":
    raise SystemExit(main())
