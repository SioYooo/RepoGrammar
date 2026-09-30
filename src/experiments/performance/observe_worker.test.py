"""Offline source-free, streaming and delegation regressions for the observer."""

import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[3]
OBSERVER = Path(__file__).with_name("observe_worker.py")
WORKER = ROOT / "src/workers/python/worker.py"
spec = importlib.util.spec_from_file_location("observe_worker", OBSERVER)
observer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(observer)


class ObserverTests(unittest.TestCase):
    def test_streaming_binary_and_text_do_not_require_eof(self):
        event = observer.Observation()
        wire = b'{"mode":"extract_interface","text":"private"}\n'
        binary = observer.ObservedBinary(io.BytesIO(wire + wire), event)
        self.assertEqual(binary.readline(), wire)
        self.assertEqual(event.counters["input_frames"], 1)
        self.assertEqual(binary.read(), wire)
        self.assertEqual(event.counters["input_frames"], 2)
        text = observer.ObservedText(io.TextIOWrapper(io.BytesIO(wire)), event)
        self.assertEqual(text.readline(), wire.decode())
        self.assertEqual(event.counters["input_bytes"], len(wire) * 3)

    def test_bounded_frames_and_unknown_modes_are_source_free(self):
        event = observer.Observation()
        event.input(b"x" * (observer.MAX_FRAME_BYTES + 1))
        self.assertEqual(len(event.pending), 0)
        event.input(b'\n{"mode":[],"text":"SENSITIVE"}\nnot-json\n')
        self.assertEqual(event.counters["oversized_frames"], 1)
        self.assertEqual(event.counters["malformed_frames"], 1)
        self.assertEqual(event.requests_by_mode, {"other": 1})
        self.assertNotIn("SENSITIVE", json.dumps(event.counters))

    def test_counts_context_repetitions_without_retaining_payload(self):
        event = observer.Observation()
        wire = json.dumps({"mode": "parse_document", "module_files": [{"path": "private.py", "text": "α"}], "conftest_files": [{"path": "conftest.py", "text": "β"}]}).encode() + b"\n"
        event.input(wire[:17])
        event.input(wire[17:] + wire)
        self.assertEqual(event.counters["module_context_source_bytes"], 4)
        self.assertEqual(event.counters["conftest_context_source_bytes"], 4)
        self.assertEqual(event.counters["module_context_records"], 2)
        self.assertEqual(len(event.pending), 0)
        output = json.dumps(event.__dict__, default=str)
        self.assertNotIn("private.py", output)
        self.assertNotIn("α", output)

    def test_session_headers_control_context_omission_without_repeated_arrays(self):
        event = observer.Observation()
        frames = [
            {"message_type": "start"},
            {"module_files": [{"path": "app.py", "text": "private"}]},
            {"message_type": "parse", "use_context": True},
            {"mode": "parse_document", "text": "private"},
            {"message_type": "parse", "use_context": False},
            {"mode": "parse_document", "text": "private"},
            {"message_type": "finish"},
        ]
        for frame in frames:
            wire = json.dumps(frame).encode() + b"\n"
            event.input(wire[:3])
            event.input(wire[3:])
        self.assertEqual(event.counters["parse_requests_without_context"], 1)
        self.assertEqual(event.counters["module_context_records"], 1)
        self.assertEqual(event.requests_by_mode["session_start"], 1)
        self.assertEqual(event.requests_by_mode["session_parse"], 2)
        self.assertEqual(event.requests_by_mode["session_end"], 1)
        event.input(b'{"mode":"parse_document","text":"private"}\n')
        self.assertEqual(event.counters["parse_requests_without_context"], 2)

    def test_real_worker_output_and_exit_are_preserved(self):
        text = "def private_sentinel():\n    return 1\n"
        payload = {"protocol_version": 1, "contract_revision": 2, "mode": "parse_document", "path": "app.py", "content_hash": "sha256:" + hashlib.sha256(text.encode()).hexdigest(), "repository_revision": "UNKNOWN", "text": text, "module_paths": ["app.py"], "source_roots": [], "module_files": [{"path": "app.py", "text": text}], "conftest_files": []}
        wire = json.dumps(payload).encode() + b"\n"
        direct = subprocess.run([sys.executable, str(WORKER)], input=wire, capture_output=True, timeout=10)
        with tempfile.TemporaryDirectory() as tmp:
            sink = Path(tmp) / "counts.jsonl"
            environment = dict(os.environ, REPOGRAMMAR_OBSERVED_WORKER=str(WORKER), REPOGRAMMAR_OBSERVER_OUT=str(sink))
            observed = subprocess.run([sys.executable, str(OBSERVER)], env=environment, input=wire, capture_output=True, timeout=10)
            self.assertEqual(observed.returncode, direct.returncode)
            self.assertEqual(observed.stdout, direct.stdout)
            report = json.loads(sink.read_text())
            self.assertEqual(report["counters"]["input_bytes"], len(wire))
            self.assertEqual(report["counters"]["output_bytes"], len(direct.stdout))
            self.assertEqual(report["counters"]["worker_spawns"], 1)
            self.assertEqual(report["counters"]["ast_parse_calls"], 3)
            self.assertNotIn("private_sentinel", sink.read_text())
            self.assertNotIn("app.py", sink.read_text())


if __name__ == "__main__":
    unittest.main()
