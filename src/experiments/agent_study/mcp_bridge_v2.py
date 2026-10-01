"""Bounded one-host MCP bridge; the product stays outside the agent sandbox.

The client shim is an NDJSON stdio transport to one coordinator-owned Unix
socket. This module starts no agent, performs no remote request and reads no
credentials. A bridge proves only local transport, not host tool enforcement.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import select
import signal
import socket
import subprocess
import sys
import threading
import time
import unicodedata


ROOT = Path(__file__).resolve().parents[3]
MAX_LINE_BYTES = 1_048_576  # Same inclusive NDJSON request bound as product serve.
MAX_FRAMES = 128
MAX_TRAFFIC_BYTES = 32 * MAX_LINE_BYTES
METHODS = ("initialize", "notifications/initialized", "ping", "tools/list", "tools/call")
OPERATIONS = ("find_analogues", "show_family", "explain_deviation", "check_conformance", "inspect_readiness")


class Refusal(ValueError):
    """Only fixed codes cross the public diagnostic boundary."""


def _object(pairs: list[tuple[str, object]]) -> dict:
    result = {}
    for key, value in pairs:
        if key in result:
            raise Refusal("DUPLICATE_KEY")
        result[key] = value
    return result


def _json(line: bytes) -> dict:
    if not line.endswith(b"\n") or not 0 < len(line) <= MAX_LINE_BYTES:
        raise Refusal("LINE_BOUND")
    try:
        value = json.loads(line, object_pairs_hook=_object,
                           parse_constant=lambda _: (_ for _ in ()).throw(Refusal("INVALID_JSON")))
    except (UnicodeError, json.JSONDecodeError, RecursionError, ValueError) as error:
        if isinstance(error, Refusal):
            raise
        raise Refusal("INVALID_JSON") from None
    if not isinstance(value, dict) or value.get("jsonrpc") != "2.0":
        raise Refusal("RPC_ENVELOPE")
    return value


def _text(value: object, maximum: int = 128) -> bool:
    try:
        return (isinstance(value, str) and bool(value.strip())
                and len(value.encode("utf-8")) <= maximum
                and not any(unicodedata.category(c) == "Cc" for c in value))
    except UnicodeError:
        return False


def _id(value: object) -> bool:
    return ((type(value) is int and -(2**63) <= value < 2**63) or _text(value))


def _meta(value: object) -> bool:
    return (isinstance(value, dict) and set(value) <= {"progressToken"}
            and ("progressToken" not in value or _id(value["progressToken"])))


def admitted_request(line: bytes) -> dict:
    """Closed study schema, checked again at the harness-owned socket boundary."""
    request = _json(line)
    if set(request) - {"jsonrpc", "id", "method", "params"}:
        raise Refusal("RPC_ENVELOPE")
    method = request.get("method")
    if method not in METHODS:
        raise Refusal("METHOD_DENIED")
    if method == "notifications/initialized":
        if "id" in request:
            raise Refusal("BAD_ID")
    elif "id" not in request or not _id(request["id"]):
        raise Refusal("BAD_ID")
    params = request.get("params", {})
    if not isinstance(params, dict):
        raise Refusal("PARAMS_DENIED")
    if "_meta" in params and not _meta(params["_meta"]):
        raise Refusal("PARAMS_DENIED")
    if method == "initialize":
        if (set(params) - {"protocolVersion", "capabilities", "clientInfo", "_meta"}
                or ("protocolVersion" in params and not _text(params["protocolVersion"]))
                or ("capabilities" in params and not isinstance(params["capabilities"], dict))):
            raise Refusal("PARAMS_DENIED")
        if "clientInfo" in params:
            info = params["clientInfo"]
            # Observed native CLI metadata is bounded and never used for I/O.
            if (not isinstance(info, dict) or not {"name", "version"} <= set(info)
                    or set(info) - {"name", "version", "title", "description", "websiteUrl"}
                    or not all(_text(v) for v in info.values())):
                raise Refusal("PARAMS_DENIED")
    elif method in ("notifications/initialized", "ping", "tools/list"):
        if set(params) - {"_meta"}:
            raise Refusal("PARAMS_DENIED")
    else:
        if (set(params) - {"name", "arguments", "_meta"}
                or params.get("name") != "repogrammar_context"):
            raise Refusal("TOOL_DENIED")
        args = params.get("arguments")
        allowed = {"operation", "target", "against", "within", "token_budget", "mode", "verbosity",
                   "include_variations", "include_exceptions", "include_source_spans"}
        if not isinstance(args, dict) or set(args) - allowed or args.get("operation") not in OPERATIONS:
            raise Refusal("ARGUMENTS_DENIED")
        for key in ("target", "against", "within"):
            if key in args and args[key] is not None and not _text(args[key], 8192):
                raise Refusal("ARGUMENTS_DENIED")
        if args.get("against") is not None and args["operation"] not in ("explain_deviation", "check_conformance"):
            raise Refusal("ARGUMENTS_DENIED")
        if "token_budget" in args and (type(args["token_budget"]) is not int or not 1 <= args["token_budget"] <= 200000):
            raise Refusal("ARGUMENTS_DENIED")
        for key, choices in (("mode", ("compact", "evidence", "deep")), ("verbosity", ("minimal", "standard", "full"))):
            if args.get(key) is not None and args[key] not in choices:
                raise Refusal("ARGUMENTS_DENIED")
        for key in ("include_variations", "include_exceptions", "include_source_spans"):
            if key in args and type(args[key]) is not bool:
                raise Refusal("ARGUMENTS_DENIED")
    return request


def _remaining(deadline: float) -> float:
    remaining = deadline - time.monotonic()
    if remaining <= 0:
        raise Refusal("DEADLINE")
    return remaining


def _write(fd: int, data: bytes, deadline: float) -> None:
    os.set_blocking(fd, False)
    view = memoryview(data)
    while view:
        if not select.select([], [fd], [], _remaining(deadline))[1]:
            raise Refusal("DEADLINE")
        try:
            written = os.write(fd, view)
        except BlockingIOError:
            continue
        view = view[written:]


def _product_line(fd: int, buffered: bytearray, deadline: float, *, allow_clean_eof: bool = False) -> bytes | None:
    """One bounded fd reader; only stdin may end cleanly between frames."""
    while b"\n" not in buffered:
        if len(buffered) > MAX_LINE_BYTES:
            raise Refusal("LINE_BOUND" if allow_clean_eof else "PRODUCT_LINE_BOUND")
        if not select.select([fd], [], [], _remaining(deadline))[0]:
            raise Refusal("DEADLINE")
        try:
            block = os.read(fd, min(65536, MAX_LINE_BYTES + 1 - len(buffered)))
        except BlockingIOError:
            continue
        if not block:
            if allow_clean_eof and not buffered:
                return None
            if allow_clean_eof:
                raise Refusal("TRUNCATED_LINE")
            raise Refusal("PRODUCT_EOF")
        buffered.extend(block)
    end = buffered.index(b"\n") + 1
    if end > MAX_LINE_BYTES:
        raise Refusal("LINE_BOUND" if allow_clean_eof else "PRODUCT_LINE_BOUND")
    line = bytes(buffered[:end])
    del buffered[:end]
    return line


def _sha_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1_048_576), b""):
            digest.update(block)
    return digest.hexdigest()


class Bridge:
    """One pinned product, one accepted client, no reconnect or live-agent API."""

    def __init__(self, binary: Path, worktree: Path, socket_path: Path, output_dir: Path,
                 *, timeout: float = 60):
        if not 0 < timeout <= 600:
            raise ValueError("timeout must be within (0, 600]")
        self.binary, self.worktree = Path(binary), Path(worktree)
        self.socket_path, self.output_dir = Path(socket_path), Path(output_dir)
        for path in (self.binary, self.worktree, self.socket_path, self.output_dir):
            if path.is_symlink() or not path.is_absolute():
                raise ValueError("bridge paths must be absolute and not symlinked")
        self.binary, self.worktree, self.socket_path, self.output_dir = (
            p.resolve() for p in (self.binary, self.worktree, self.socket_path, self.output_dir))
        if not self.binary.is_file() or not os.access(self.binary, os.X_OK):
            raise ValueError("product must be a pinned executable regular file")
        if not self.worktree.is_dir() or self.worktree == ROOT or ROOT in self.worktree.parents:
            raise ValueError("product worktree must be an outside-repository directory")
        if (self.output_dir == ROOT or ROOT in self.output_dir.parents
                or self.output_dir == self.worktree or self.worktree in self.output_dir.parents
                or self.socket_path == self.worktree or self.worktree in self.socket_path.parents
                or self.socket_path == ROOT or ROOT in self.socket_path.parents):
            raise ValueError("bridge/private artifacts must be outside agent worktree and repository")
        if self.output_dir.exists() or self.socket_path.exists():
            raise ValueError("bridge output and socket must be new")
        self.timeout = timeout
        self.summary = {"schema_version": "agent-adoption-mcp-bridge.v2", "status": "NOT_STARTED",
                        "reason": None, "binary_sha256": _sha_file(self.binary),
                        "bridge_source_sha256": _sha_file(Path(__file__)),
                        "method_counts": {m: 0 for m in METHODS}, "frames": [],
                        "request_bytes": 0, "result_bytes": 0,
                        "requests_sha256": None, "results_sha256": None,
                        "connections": 0, "product_exit_code": None, "process_cleanup": "NOT_MEASURED"}
        self.process = None
        self.listener = self.connection = self.thread = None
        self._stopped = threading.Event()
        self._socket_identity = None

    def start(self) -> "Bridge":
        if self.summary["status"] != "NOT_STARTED":
            raise ValueError("bridge starts once")
        self.output_dir.mkdir(parents=True, mode=0o700)
        self.summary["status"] = "STARTING"
        try:
            if _sha_file(self.binary) != self.summary["binary_sha256"]:
                raise Refusal("BINARY_DRIFT")
            home = self.output_dir / "home"
            home.mkdir(mode=0o700)
            env = {"PATH": "/usr/bin:/bin", "HOME": str(home), "CODEX_HOME": str(home / ".codex"),
                   "CLAUDE_CONFIG_DIR": str(home / ".claude"), "XDG_CONFIG_HOME": str(home / ".config"),
                   "XDG_CACHE_HOME": str(home / ".cache"), "XDG_DATA_HOME": str(home / ".local/share"),
                   "XDG_STATE_HOME": str(home / ".local/state"), "TMPDIR": str(self.output_dir)}
            self.listener = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            self.listener.bind(str(self.socket_path))
            os.chmod(self.socket_path, 0o600)
            metadata = self.socket_path.stat()
            self._socket_identity = (metadata.st_dev, metadata.st_ino)
            self.listener.listen(1)
            self.listener.settimeout(0.2)
            self.process = subprocess.Popen([str(self.binary), "serve", "--project", str(self.worktree)],
                                            cwd=self.worktree, env=env, stdin=subprocess.PIPE,
                                            stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, bufsize=0,
                                            start_new_session=True)
            self.summary["status"] = "RUNNING"
            self.thread = threading.Thread(target=self._serve, daemon=True)
            self.thread.start()
        except Exception:
            self.summary["status"], self.summary["reason"] = "START_FAILURE", "START_FAILURE"
            self.close()
            raise
        return self

    def _serve(self) -> None:
        deadline = time.monotonic() + self.timeout
        buffered, seen = bytearray(), set()
        try:
            while not self._stopped.is_set():
                _remaining(deadline)
                try:
                    self.connection, _ = self.listener.accept()
                    break
                except socket.timeout:
                    continue
            if self.connection is None:
                raise Refusal("COORDINATOR_CLOSE")
            self.summary["connections"] = 1
            self.listener.close()  # A second host cannot reconnect to this product.
            self.listener = None
            with (self.output_dir / "requests.jsonl").open("xb") as requests, (
                    self.output_dir / "results.jsonl").open("xb") as results, self.connection.makefile("rb") as incoming:
                while not self._stopped.is_set():
                    self.connection.settimeout(_remaining(deadline))
                    line = incoming.readline(MAX_LINE_BYTES + 1)
                    if not line:
                        self.summary["status"] = "CLOSED" if self._stopped.is_set() else "COMPLETE"
                        if self._stopped.is_set():
                            self.summary["reason"] = "COORDINATOR_CLOSE"
                        break
                    requests.write(line)
                    requests.flush()
                    self.summary["request_bytes"] += len(line)
                    if self.summary["request_bytes"] + self.summary["result_bytes"] > MAX_TRAFFIC_BYTES:
                        raise Refusal("TRAFFIC_BOUND")
                    if len(self.summary["frames"]) >= MAX_FRAMES:
                        raise Refusal("FRAME_BOUND")
                    request = admitted_request(line)
                    if "id" in request:
                        identifier = (type(request["id"]), request["id"])
                        if identifier in seen:
                            raise Refusal("ID_REUSE")
                        seen.add(identifier)
                    method = request["method"]
                    row = {"ordinal": len(self.summary["frames"]), "method": method,
                           "request_sha256": hashlib.sha256(line).hexdigest(),
                           "result_sha256": None, "status": "ADMITTED"}
                    self.summary["frames"].append(row)
                    self.summary["method_counts"][method] += 1
                    _write(self.process.stdin.fileno(), line, deadline)
                    if method == "notifications/initialized":
                        row["status"] = "NOTIFICATION_FORWARDED"
                        continue
                    response = _product_line(self.process.stdout.fileno(), buffered, deadline)
                    results.write(response)
                    results.flush()
                    self.summary["result_bytes"] += len(response)
                    if self.summary["request_bytes"] + self.summary["result_bytes"] > MAX_TRAFFIC_BYTES:
                        raise Refusal("TRAFFIC_BOUND")
                    payload = _json(response)
                    if (set(payload) - {"jsonrpc", "id", "result", "error"}
                            or type(payload.get("id")) is not type(request["id"])
                            or payload.get("id") != request["id"]
                            or ("result" in payload) == ("error" in payload)):
                        raise Refusal("PRODUCT_RESPONSE")
                    row["result_sha256"] = hashlib.sha256(response).hexdigest()
                    row["status"] = "PRODUCT_ERROR" if "error" in payload else "PRODUCT_SUCCESS"
                    self.connection.settimeout(_remaining(deadline))
                    self.connection.sendall(response)
        except (Refusal, OSError, ValueError) as error:
            if self._stopped.is_set():
                self.summary["status"], self.summary["reason"] = "CLOSED", "COORDINATOR_CLOSE"
            else:
                self.summary["status"] = "FAILED"
                self.summary["reason"] = str(error) if isinstance(error, Refusal) else "TRANSPORT_FAILURE"
        finally:
            self._cleanup()

    def _cleanup(self) -> None:
        for endpoint in (self.connection, self.listener):
            if endpoint is not None:
                endpoint.close()
        if self.process is not None:
            if self.process.stdin is not None:
                self.process.stdin.close()
            try:
                self.process.wait(timeout=1)
            except subprocess.TimeoutExpired:
                os.killpg(self.process.pid, signal.SIGTERM)
                try:
                    self.process.wait(timeout=1)
                except subprocess.TimeoutExpired:
                    os.killpg(self.process.pid, signal.SIGKILL)
                    self.process.wait(timeout=1)
            if self.process.stdout is not None:
                self.process.stdout.close()
            self.summary["product_exit_code"] = self.process.returncode
            self.summary["process_cleanup"] = "PASS"
        if self._socket_identity is not None and self.socket_path.exists():
            metadata = self.socket_path.lstat()
            if (metadata.st_dev, metadata.st_ino) == self._socket_identity:
                self.socket_path.unlink()
        if self.output_dir.exists():
            for filename, key in (("requests.jsonl", "requests_sha256"), ("results.jsonl", "results_sha256")):
                path = self.output_dir / filename
                if path.exists():
                    self.summary[key] = _sha_file(path)
            (self.output_dir / "bridge.summary.json").write_text(json.dumps(self.summary, indent=2, sort_keys=True) + "\n")

    def close(self) -> dict:
        self._stopped.set()
        if self.connection is not None:
            try:
                self.connection.shutdown(socket.SHUT_RDWR)
            except OSError:
                pass
        if self.listener is not None:
            self.listener.close()
        if self.thread is not None and self.thread is not threading.current_thread():
            self.thread.join(timeout=3)
            if self.thread.is_alive():
                if self.process is not None and self.process.poll() is None:
                    os.killpg(self.process.pid, signal.SIGKILL)
                self.thread.join(timeout=2)
        if self.thread is None:
            self._cleanup()
        return self.summary

    def __enter__(self) -> "Bridge":
        return self.start()

    def __exit__(self, *unused: object) -> None:
        self.close()


def _audit_stream(path: Path, repository_root: Path | None = None):
    path = Path(path)
    resolved = path.resolve()
    source_root = ROOT if (ROOT / ".git").exists() and (ROOT / "AGENTS.md").is_file() else None
    if repository_root is None:
        if source_root is None:
            raise Refusal("AUDIT_AUTHORITY")
        repository_root = source_root
    if not Path(repository_root).is_absolute():
        raise Refusal("AUDIT_AUTHORITY")
    # A copied client receives the coordinator's frozen canonical root. Do not
    # infer its runtime directory as a repository or read unrelated ancestors.
    guard = Path(os.path.normpath(str(repository_root)))
    roots = (guard,) if source_root is None else (guard, source_root)
    if (not path.is_absolute() or path.is_symlink()
            or any(resolved == root or root in resolved.parents for root in roots)):
        raise Refusal("AUDIT_PATH")
    return os.fdopen(os.open(resolved, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600), "wb")


def client(socket_path: Path, *, timeout: float = 60, audit_file: Path | None = None,
           repository_root: Path | None = None) -> int:
    """Connect to a fixed socket; preserve product NDJSON and notification silence."""
    if not Path(socket_path).is_absolute() or not 0 < timeout <= 600:
        raise ValueError("absolute socket path and bounded timeout required")
    # Some launchers alias duplex stdin/stdout. Both directions use bounded fd
    # I/O, so stdout's O_NONBLOCK cannot turn stdin EAGAIN into buffered EOF.
    os.set_blocking(sys.stdin.fileno(), False)
    deadline = time.monotonic() + timeout
    failure = []
    pending = [0]
    state_lock = threading.Lock()
    input_complete = threading.Event()
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as connection:
        connection.settimeout(_remaining(deadline))
        connection.connect(str(socket_path))
        audit = _audit_stream(audit_file, repository_root) if audit_file is not None else None

        def send() -> None:
            frames = traffic = 0
            buffered = bytearray()
            try:
                while True:
                    line = _product_line(sys.stdin.fileno(), buffered, deadline, allow_clean_eof=True)
                    if line is None:
                        input_complete.set()
                        connection.shutdown(socket.SHUT_WR)
                        return
                    frames += 1
                    traffic += len(line)
                    if frames > MAX_FRAMES:
                        raise Refusal("FRAME_BOUND")
                    if traffic > MAX_TRAFFIC_BYTES:
                        raise Refusal("TRAFFIC_BOUND")
                    if audit is not None:
                        audit.write(line)
                        audit.flush()
                    request = admitted_request(line)
                    if request["method"] != "notifications/initialized":
                        with state_lock:
                            pending[0] += 1
                    connection.settimeout(_remaining(deadline))
                    connection.sendall(line)
            except (OSError, ValueError) as error:
                failure.append(error)
                reason = str(error) if isinstance(error, Refusal) else "TRANSPORT_FAILURE"
                print("MCP_BRIDGE_CLIENT_FAILURE:" + reason, file=sys.stderr, flush=True)
                try:
                    connection.shutdown(socket.SHUT_RDWR)
                except OSError:
                    pass
            finally:
                if audit is not None:
                    audit.close()

        threading.Thread(target=send, daemon=True).start()
        with connection.makefile("rb") as incoming:
            while True:
                connection.settimeout(_remaining(deadline))
                line = incoming.readline(MAX_LINE_BYTES + 1)
                if not line:
                    with state_lock:
                        return 0 if input_complete.is_set() and pending[0] == 0 and not failure else 1
                _json(line)
                with state_lock:
                    pending[0] -= 1
                _write(sys.stdout.fileno(), line, deadline)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--socket", required=True, type=Path)
    parser.add_argument("--timeout", type=float, default=60)
    parser.add_argument("--audit-file", type=Path)
    parser.add_argument("--repository-root", type=Path)
    args = parser.parse_args()
    try:
        raise SystemExit(client(args.socket, timeout=args.timeout, audit_file=args.audit_file,
                                repository_root=args.repository_root))
    except (OSError, ValueError) as error:
        reason = str(error) if isinstance(error, Refusal) else "TRANSPORT_FAILURE"
        print("MCP_BRIDGE_CLIENT_FAILURE:" + reason, file=sys.stderr)
        raise SystemExit(1)
