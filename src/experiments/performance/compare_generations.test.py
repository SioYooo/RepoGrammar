"""Offline negatives for strict whole-generation A/B comparison."""

from contextlib import closing
import importlib.util
import json
from pathlib import Path
import sqlite3
import tempfile
import unittest


spec = importlib.util.spec_from_file_location("comparison", Path(__file__).with_name("compare_generations.py"))
comparison = importlib.util.module_from_spec(spec)
spec.loader.exec_module(comparison)


def fixture(path, generation, value):
    with closing(sqlite3.connect(path)) as db, db:
        db.execute("CREATE TABLE index_generations (generation_id TEXT, status TEXT, repogrammar_version TEXT, repository_revision TEXT, worktree_hash TEXT, created_at TEXT)")
        db.execute("INSERT INTO index_generations VALUES (?, 'active', '0.5.0', 'UNKNOWN', 'hash', ?)", (generation, generation))
        db.execute("CREATE TABLE schema_migrations (version INTEGER, name TEXT, applied_at TEXT)")
        db.execute("INSERT INTO schema_migrations VALUES (1, 'initial', ?)", (generation,))
        for table in comparison.TABLES:
            db.execute(f'CREATE TABLE "{table}" (generation_id TEXT, payload_json TEXT, FOREIGN KEY (generation_id) REFERENCES index_generations(generation_id))')
        # SQLite FK validation requires a unique parent key.
        db.execute("CREATE UNIQUE INDEX generation_key ON index_generations(generation_id)")
        db.execute("INSERT INTO semantic_facts VALUES (?, ?)", (generation, value))


class ComparisonTests(unittest.TestCase):
    def test_duplicate_json_claims_cannot_compare_equal_after_last_wins_decoding(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "fixture.db"
            fixture(path, "one", '{"claim":"contradiction","claim":"supported"}')
            with self.assertRaisesRegex(ValueError, "duplicate JSON"):
                comparison.fingerprint(path)

    def test_order_and_clock_independence_but_same_count_fact_change_fails(self):
        with tempfile.TemporaryDirectory() as tmp:
            left, right = Path(tmp) / "left.db", Path(tmp) / "right.db"
            fixture(left, "generation-one", '{"b":2,"a":"PRIVATE_SOURCE"}')
            fixture(right, "generation-two", '{"a":"PRIVATE_SOURCE", "b":2}')
            before = comparison.fingerprint(left)
            self.assertEqual(before, comparison.fingerprint(right))
            self.assertNotIn("PRIVATE_SOURCE", json.dumps(before))
            with closing(sqlite3.connect(right)) as db, db:
                db.execute("UPDATE semantic_facts SET payload_json='{}'")
            after = comparison.fingerprint(right)
            self.assertEqual(before["tables"]["semantic_facts"]["rows"], after["tables"]["semantic_facts"]["rows"])
            self.assertNotEqual(before, after)

    def test_family_evidence_and_unqualified_tables_cannot_be_omitted(self):
        with tempfile.TemporaryDirectory() as tmp:
            left, right = Path(tmp) / "left.db", Path(tmp) / "right.db"
            fixture(left, "one", '{}')
            fixture(right, "two", '{}')
            with closing(sqlite3.connect(right)) as db, db:
                db.execute("INSERT INTO family_members VALUES ('two', '{}')")
            self.assertNotEqual(comparison.fingerprint(left), comparison.fingerprint(right))
            with closing(sqlite3.connect(right)) as db, db:
                db.execute("CREATE TABLE unqualified (x INTEGER)")
            with self.assertRaisesRegex(ValueError, "table set"):
                comparison.fingerprint(right)

    def test_canonical_multiset_order_is_independent_of_raw_json_sql_sort(self):
        with tempfile.TemporaryDirectory() as tmp:
            left, right = Path(tmp) / "left.db", Path(tmp) / "right.db"
            fixture(left, "one", '{"a":1,"b":2}')
            fixture(right, "two", '{"b":2,"a":1}')
            with closing(sqlite3.connect(left)) as db, db:
                db.execute("INSERT INTO semantic_facts VALUES ('one', ?)", ('{"a":2,"b":1}',))
            with closing(sqlite3.connect(right)) as db, db:
                db.execute("INSERT INTO semantic_facts VALUES ('two', ?)", ('{"b":1,"a":2}',))
            self.assertEqual(comparison.fingerprint(left), comparison.fingerprint(right))
            with closing(sqlite3.connect(right)) as db, db:
                db.execute("INSERT INTO semantic_facts VALUES ('two', ?)", ('{"b":1,"a":2}',))
            self.assertNotEqual(comparison.fingerprint(left), comparison.fingerprint(right))

    def test_missing_active_generation_invalid_json_and_foreign_keys_fail(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "fixture.db"
            fixture(path, "one", '{}')
            with closing(sqlite3.connect(path)) as db, db:
                db.execute("UPDATE semantic_facts SET payload_json='malformed'")
            with self.assertRaises(ValueError):
                comparison.fingerprint(path)
            with closing(sqlite3.connect(path)) as db, db:
                db.execute("UPDATE semantic_facts SET payload_json='{}', generation_id='missing'")
            with self.assertRaisesRegex(ValueError, "foreign keys"):
                comparison.fingerprint(path)
            with closing(sqlite3.connect(path)) as db, db:
                db.execute("DELETE FROM semantic_facts")
                db.execute("UPDATE index_generations SET status='failed'")
            with self.assertRaisesRegex(ValueError, "active generation"):
                comparison.fingerprint(path)


if __name__ == "__main__":
    unittest.main()
