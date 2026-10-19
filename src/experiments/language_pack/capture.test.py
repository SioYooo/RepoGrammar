import json
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))
from capture import canonical, fixture_roots, mcp_payload

ROOT = Path(__file__).resolve().parents[3]


class CaptureTest(unittest.TestCase):
    def test_canonical_drops_only_generation_identity_and_keeps_order(self):
        value = {"active_generation": "gen-1", "units": [{"generation_id": "gen-1", "id": "b"}, {"id": "a"}],
                 "status": "ok"}
        self.assertEqual(canonical(value), {"units": [{"id": "b"}, {"id": "a"}], "status": "ok"})

    def test_fixture_roots_cover_csharp_and_incremental_fixtures(self):
        roots = {str(path.relative_to(ROOT)) for path in fixture_roots(ROOT / "src/fixtures")}
        self.assertIn("src/fixtures/csharp/release/v0_2/aspnet_exact_controllers", roots)
        self.assertIn("src/fixtures/unknown_reduction/csharp_aspnet_unresolved", roots)
        self.assertIn("src/fixtures/incremental_equivalence/v1", roots)

    def test_mcp_payload_requires_exactly_one_tool_response(self):
        text = json.dumps({"status": "FOUND"})
        response = {"jsonrpc": "2.0", "id": 1, "result": {"content": [{"type": "text", "text": text}]}}
        stdout = (json.dumps({"jsonrpc": "2.0", "id": 0, "result": {}}) + "\n" + json.dumps(response) + "\n").encode()
        self.assertEqual(mcp_payload(stdout), {"status": "FOUND"})
        with self.assertRaises(ValueError):
            mcp_payload(stdout + json.dumps(response).encode() + b"\n")


if __name__ == "__main__":
    unittest.main()
