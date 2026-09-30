"""Offline regressions for warm profile framing and whole-result parity."""

import os
import time
import unittest

import warm_queries


class WarmProfileTests(unittest.TestCase):
    def test_metadata_only_span_stub_is_not_source_but_actual_spans_are_rejected(self):
        warm_queries.require_source_free({"output": {"source_snippets_included": False}, "source_spans": {"requested": False, "source_snippets_included": False, "spans": [], "omissions": []}})
        for payload in [
            {"output": {"source_snippets_included": True}},
            {"source_spans": {"source_snippets_included": True, "spans": []}},
            {"source_spans": {"source_snippets_included": False, "spans": [{"text": "PRIVATE"}]}},
        ]:
            with self.assertRaises(ValueError):
                warm_queries.require_source_free(payload)

    def test_partial_message_cannot_block_past_deadline(self):
        read, write = os.pipe()
        try:
            os.write(write, b'{"partial":')
            started = time.monotonic()
            with self.assertRaises(TimeoutError):
                warm_queries.read_frame(read, bytearray(), started + .05)
            self.assertLess(time.monotonic() - started, 1)
        finally:
            os.close(read)
            os.close(write)

    def test_frame_bounds_eof_and_retained_next_message(self):
        read, write = os.pipe()
        try:
            os.write(write, b'{}\n{}\n')
            pending = bytearray()
            self.assertEqual(warm_queries.read_frame(read, pending, time.monotonic() + 1), b'{}\n')
            self.assertEqual(warm_queries.read_frame(read, pending, time.monotonic() + 1), b'{}\n')
            os.write(write, b'x' * 1025)
            with self.assertRaises(ValueError):
                warm_queries.read_frame(read, pending, time.monotonic() + 1, 1024)
            os.close(write)
            write = None
            with self.assertRaises(ValueError):
                warm_queries.read_frame(read, bytearray(), time.monotonic() + 1)
        finally:
            os.close(read)
            if write is not None:
                os.close(write)

    def test_first_bad_last_good_and_same_size_content_drift_fail(self):
        first = warm_queries.accept_sample(None, {"status": "UNKNOWN", "read_plan": []}, 20)
        with self.assertRaises(ValueError):
            warm_queries.accept_sample(first, {"status": "ok", "read_plan": []}, 20)
        with self.assertRaises(ValueError):
            warm_queries.accept_sample(first, {"status": "UNKNOWN", "read_plan": [1]}, 20)
        reordered = warm_queries.accept_sample(None, {"b": 2, "a": 1}, 10)
        self.assertEqual(reordered, warm_queries.accept_sample(reordered, {"a": 1, "b": 2}, 10))
        with self.assertRaises(ValueError):
            warm_queries.accept_sample(reordered, {"a": 1, "b": 2}, 11)


if __name__ == "__main__":
    unittest.main()
