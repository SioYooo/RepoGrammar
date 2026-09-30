"""Executable offline checks; all source-bearing products stay in temporary dirs."""

from __future__ import annotations

import copy
import json
from pathlib import Path
import stat
import tempfile
import unittest

from oracles_v2 import ROOT, assess, frozen_tasks, qualify, screen


class OracleQualificationTests(unittest.TestCase):
    def test_eight_references_and_twenty_four_mutants(self):
        tasks = frozen_tasks()
        self.assertEqual(len(tasks), 8)
        self.assertEqual(sum(t["eligible"] for t in tasks), 6)
        for task in tasks:
            reference = task["reference"]
            with self.subTest(task=task["kind"], bundle="reference"):
                self.assertEqual(assess(task, task["seed"] | reference["files"], reference["answer"])["status"], "PASS")
            for mutant in task["mutants"]:
                with self.subTest(task=task["kind"], bundle=mutant["kind"]):
                    self.assertEqual(assess(task, task["seed"] | mutant["files"], mutant["answer"])["status"], "FAIL")

    def test_scope_and_existing_analogue_do_not_presatisfy_additions(self):
        for task in frozen_tasks()[:4]:
            self.assertEqual(assess(task, task["seed"], {})["status"], "FAIL")
            reference = task["reference"]
            files = task["seed"] | reference["files"]
            files["other.py"] = "injected = True\n"
            self.assertEqual(assess(task, files, {})["scope"], "FAIL")
            target = next(iter(reference["files"]))
            files = task["seed"] | reference["files"]
            files[target] = files[target].replace("from ", "# omitted import\nfrom ", 1)
            # Comments are immaterial; AST preservation admits normal formatting.
            self.assertEqual(assess(task, files, {})["status"], "PASS")
            files[target] = files[target].replace("from ", "import invented\nfrom ", 1)
            self.assertEqual(assess(task, files, {})["status"], "FAIL")

    def test_static_oracle_rejects_independent_wrong_behavior(self):
        mutations = [
            ("labels_for(package_code)", "weight_for(package_code)"),
            ("bundle.close()", "pass"),
            ("value.strip().upper()", "value.strip().lower()"),
            ("self.session.scalars(stmt)", "self.session.execute(stmt)"),
        ]
        for task, (old, new) in zip(frozen_tasks()[:4], mutations):
            files = task["seed"] | task["reference"]["files"]
            target = next(iter(task["reference"]["files"]))
            # Only mutate the new declaration; existing analogue stays intact.
            position = files[target].rfind(old)
            self.assertGreaterEqual(position, 0)
            files[target] = files[target][:position] + files[target][position:].replace(old, new, 1)
            self.assertEqual(assess(task, files, {})["status"], "FAIL")

    def test_untrusted_code_is_parsed_only(self):
        task = frozen_tasks()[0]
        files = task["seed"] | task["reference"]["files"]
        files["app/catalog.py"] += "\nraise RuntimeError('must never execute')\n"
        self.assertEqual(assess(task, files, {})["status"], "FAIL")
        files["app/catalog.py"] = "def invalid(\n"
        self.assertEqual(assess(task, files, {})["status"], "FAIL")
        with self.assertRaises(ValueError):
            assess(task, {"app/catalog.py": object()}, {})

    def test_burned_statement_and_source_screen(self):
        tasks = frozen_tasks()
        self.assertEqual(screen(tasks)["status"], "PASS")
        altered = copy.deepcopy(tasks)
        old = json.loads((ROOT / "src/experiments/agent_study/tasks/t1_item_summary.json").read_text())
        altered[0]["prompt"] = old["prompt"]
        self.assertEqual(screen(altered)["phrase_overlap_task_count"], 1)
        historical = ROOT / "src/experiments/agent_study/fixtures/mini_repo/app/routes.py"
        altered[0]["seed"]["app/catalog.py"] = historical.read_text()
        self.assertGreater(screen(altered)["exact_ast_overlap_file_count"], 0)

    def test_freeze_is_deterministic_private_and_source_free(self):
        with tempfile.TemporaryDirectory() as directory:
            first = Path(directory) / "first"
            second = Path(directory) / "second"
            a, b = qualify(first), qualify(second)
            self.assertEqual(a, b)
            self.assertEqual(a["status"], "PASS")
            self.assertEqual(a["reference_pass_count"], 8)
            self.assertEqual(a["mutant_reject_count"], 24)
            self.assertEqual(a["split"], "DEVELOPMENT_BURNED")
            self.assertEqual(a["held_out_effectiveness"], "NOT_MEASURED")
            self.assertEqual(a["runtime_auth"], "NOT_MEASURED")
            self.assertEqual(stat.S_IMODE((first / "private").stat().st_mode), 0o700)
            self.assertEqual(stat.S_IMODE((first / "private/tasks.json").stat().st_mode), 0o600)
            summary = (first / "qualification.summary.json").read_text()
            for forbidden in ("package_code", "warehouse_code", "runtime_bindings.py", directory, "prompt", "answer", "reference\":"):
                self.assertNotIn(forbidden, summary)
            private = json.loads((first / "private/tasks.json").read_text())
            self.assertEqual(private, list(frozen_tasks()))
            self.assertNotEqual(private[-1]["seed"], private[-1]["index_seed"])
            self.assertEqual((first / "private/oracle.py").read_bytes(), Path(__file__).with_name("oracles_v2.py").read_bytes())
            with self.assertRaises(ValueError):
                qualify(first)
            link = Path(directory) / "alias"
            link.symlink_to(first, target_is_directory=True)
            with self.assertRaises(ValueError):
                qualify(link)
        with self.assertRaises(ValueError):
            qualify(ROOT / "forbidden-oracle-output")

    def test_frozen_tasks_return_private_copies(self):
        task = frozen_tasks()[0]
        task["seed"]["app/catalog.py"] = "contamination"
        self.assertNotEqual(frozen_tasks()[0]["seed"]["app/catalog.py"], "contamination")


if __name__ == "__main__":
    unittest.main()
