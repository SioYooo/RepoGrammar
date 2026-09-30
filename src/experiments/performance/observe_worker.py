"""Diagnostic-only worker observer; native A/B runs do not load this file.

Delegates to a caller-selected, hashed RepoGrammar-owned worker. Never executes
project code. Retains at most one bounded input frame; records no source, paths,
hashes, symbols, query text or errors. Inclusive function timers overlap.
"""

import ast
import json
import os
import runpy
import sys
import time


MAX_FRAME_BYTES = 2 * 1024 * 1024
MODES = {
    "parse_document", "extract_interface", "parse_project_config",
    "project_context", "session_start", "session_parse", "session_end",
}


class Observation:
    def __init__(self):
        self.pending = bytearray()
        self.dropping = False
        self.counters = {
            "worker_spawns": 1, "input_bytes": 0, "output_bytes": 0,
            "input_frames": 0, "malformed_frames": 0, "oversized_frames": 0,
            "module_context_source_bytes": 0, "conftest_context_source_bytes": 0,
            "module_context_records": 0, "conftest_context_records": 0,
            "parse_requests_without_context": 0, "ast_parse_calls": 0,
        }
        self.requests_by_mode = {}
        self.function_ns = {}
        self.pending_use_context = None

    def input(self, data):
        self.counters["input_bytes"] += len(data)
        # A streaming session must never wait for EOF before delegating reads.
        for part in data.splitlines(keepends=True):
            ended = part.endswith(b"\n")
            if not self.dropping:
                if len(self.pending) + len(part) > MAX_FRAME_BYTES:
                    self.pending.clear()
                    self.dropping = True
                    self.counters["oversized_frames"] += 1
                else:
                    self.pending.extend(part)
            if ended:
                if not self.dropping:
                    self.frame(bytes(self.pending))
                self.pending.clear()
                self.dropping = False

    def frame(self, frame):
        if not frame.strip():
            return
        self.counters["input_frames"] += 1
        try:
            value = json.loads(frame)
        except (ValueError, UnicodeError, RecursionError):
            self.counters["malformed_frames"] += 1
            return
        if not isinstance(value, dict):
            self.counters["malformed_frames"] += 1
            return
        mode = value.get("mode")
        control = value.get("message_type")
        session_mode = {"start": "session_start", "parse": "session_parse", "finish": "session_end"}.get(control) if isinstance(control, str) else None
        if session_mode:
            mode = session_mode
            self.pending_use_context = value.get("use_context") if control == "parse" and type(value.get("use_context")) is bool else None
        mode = mode if isinstance(mode, str) and mode in MODES else "other"
        self.requests_by_mode[mode] = self.requests_by_mode.get(mode, 0) + 1
        context = value.get("context", value)
        if not isinstance(context, dict):
            context = {}
        if mode == "parse_document":
            omitted = not self.pending_use_context if self.pending_use_context is not None else "module_files" not in context
            self.counters["parse_requests_without_context"] += int(omitted)
            self.pending_use_context = None
        for field, prefix in (("module_files", "module"), ("conftest_files", "conftest")):
            records = context.get(field, [])
            if not isinstance(records, list):
                continue
            for record in records:
                if isinstance(record, dict) and isinstance(record.get("text"), str):
                    try:
                        size = len(record["text"].encode("utf-8"))
                    except UnicodeError:
                        self.counters["malformed_frames"] += 1
                        continue
                    self.counters[f"{prefix}_context_records"] += 1
                    self.counters[f"{prefix}_context_source_bytes"] += size

    def finish(self):
        if self.pending and not self.dropping:
            self.frame(bytes(self.pending))
        self.pending.clear()

    def timed(self, name, function):
        def wrapped(*args, **kwargs):
            started = time.perf_counter_ns()
            if name == "ast_parse":
                self.counters["ast_parse_calls"] += 1
            try:
                return function(*args, **kwargs)
            finally:
                self.function_ns[name] = self.function_ns.get(name, 0) + time.perf_counter_ns() - started
        return wrapped


class ObservedBinary:
    def __init__(self, stream, observation):
        self.stream, self.observation = stream, observation

    def read(self, size=-1):
        value = self.stream.read(size)
        self.observation.input(value)
        return value

    def readline(self, size=-1):
        value = self.stream.readline(size)
        self.observation.input(value)
        return value

    def write(self, value):
        written = self.stream.write(value)
        self.observation.counters["output_bytes"] += written
        return written

    def __getattr__(self, name):
        return getattr(self.stream, name)


class ObservedText:
    def __init__(self, stream, observation):
        self.stream, self.observation = stream, observation
        self.buffer = ObservedBinary(stream.buffer, observation)

    def read(self, size=-1):
        value = self.stream.read(size)
        self.observation.input(value.encode("utf-8"))
        return value

    def readline(self, size=-1):
        value = self.stream.readline(size)
        self.observation.input(value.encode("utf-8"))
        return value

    def write(self, value):
        written = self.stream.write(value)
        self.observation.counters["output_bytes"] += len(value[:written].encode("utf-8"))
        return written

    def __iter__(self):
        return self

    def __next__(self):
        value = self.readline()
        if not value:
            raise StopIteration
        return value

    def __getattr__(self, name):
        return getattr(self.stream, name)


def main():
    observation = Observation()
    started = time.perf_counter_ns()
    original_stdin, original_stdout, original_parse = sys.stdin, sys.stdout, ast.parse
    sys.stdin, sys.stdout = ObservedText(sys.stdin, observation), ObservedText(sys.stdout, observation)
    try:
        namespace = runpy.run_path(os.environ["REPOGRAMMAR_OBSERVED_WORKER"], run_name="__observed_worker__")
        load_ns = time.perf_counter_ns() - started
        globals_ = namespace["main"].__globals__
        ast.parse = observation.timed("ast_parse", original_parse)
        for name in ("build_module_index", "build_module_symbol_index", "conftest_fixture_index", "analyze_source", "interface_hash"):
            function = globals_.get(name)
            if callable(function):
                globals_[name] = observation.timed(name, function)
        # Preserve flags such as the future project's session switch.
        return namespace["main"]()
    finally:
        observation.finish()
        sys.stdin, sys.stdout, ast.parse = original_stdin, original_stdout, original_parse
        report = {
            "schema_version": "python-observer.v1",
            "counters": observation.counters,
            "requests_by_mode": observation.requests_by_mode,
            "diagnostic_inclusive_function_ns": observation.function_ns,
            "diagnostic_worker_ns": time.perf_counter_ns() - started,
            "diagnostic_worker_load_ns": locals().get("load_ns"),
        }
        with open(os.environ["REPOGRAMMAR_OBSERVER_OUT"], "a", encoding="utf-8") as sink:
            sink.write(json.dumps(report, separators=(",", ":"), sort_keys=True) + "\n")


if __name__ == "__main__":
    raise SystemExit(main())
