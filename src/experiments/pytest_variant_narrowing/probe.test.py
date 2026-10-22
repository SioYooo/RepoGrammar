import importlib.util
import json
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
SPEC = importlib.util.spec_from_file_location("probe", HERE / "probe.py")
probe = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(probe)


class ProbeTest(unittest.TestCase):
    def test_under_is_component_exact(self):
        self.assertTrue(probe.under("a/b/c.py", "a/b"))
        self.assertTrue(probe.under("a/b/c.py", "a/b/c.py"))
        self.assertFalse(probe.under("a/bc/c.py", "a/b"))

    def test_outcome_selects_only_on_ok(self):
        ok = probe.outcome(0, {"status": "ok", "family": {"family_id": "f"}})
        self.assertEqual(ok["selected"], "f")
        self.assertIsNone(probe.outcome(0, {"status": "UNKNOWN", "family": {"family_id": "f"}})["selected"])

    def test_samples_are_well_formed(self):
        spec = json.loads((HERE / "samples.json").read_text())
        ids = [s["id"] for s in spec["samples"]]
        self.assertEqual(len(ids), len(set(ids)))
        for sample in spec["samples"]:
            self.assertIn(sample["corpus"], spec["corpora"])
            if sample["target"].startswith("@"):
                self.assertIn(sample["target"][1:].split(" ")[0], spec["verbatim"])


if __name__ == "__main__":
    unittest.main()
