"""Strict, source-free full-generation A/B check; not an incremental oracle.

Uses SQLite's read-only snapshot and hashes every active owned row. Only the
generation identity and clock metadata are excluded. Unlike the product
sync-equivalence oracle, evidence IDs must match: use identical full-index
inputs/order. Outputs contain table names, counts and hashes, never row values.
"""

import argparse
from contextlib import closing
import hashlib
import json
from pathlib import Path
import sqlite3
import time


TABLES = {
    "indexed_files", "code_units", "ir_nodes", "ir_edges", "semantic_facts",
    "dependency_records", "families", "family_members", "variation_slots",
    "evidence", "family_constraint_profiles", "derived_record_dependencies",
    "python_module_interfaces", "dirty_records",
}


def canonical_bytes(value):
    return json.dumps(value, ensure_ascii=True, sort_keys=True, separators=(",", ":"), allow_nan=False).encode()


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError("duplicate JSON field")
        result[key] = value
    return result


def fingerprint(path):
    deadline = time.monotonic() + 60
    if Path(path).stat().st_size > 512 * 1024 * 1024:
        raise ValueError("database comparison byte budget exceeded")
    with closing(sqlite3.connect(Path(path).resolve().as_uri() + "?mode=ro", uri=True)) as db:
        db.set_progress_handler(lambda: int(time.monotonic() > deadline), 10000)
        db.execute("PRAGMA query_only=ON")
        db.execute("BEGIN")
        if db.execute("PRAGMA integrity_check").fetchall() != [("ok",)]:
            raise ValueError("database integrity failed")
        if db.execute("PRAGMA foreign_key_check").fetchone() is not None:
            raise ValueError("database foreign keys failed")
        names = {row[0] for row in db.execute("SELECT name FROM sqlite_master WHERE type='table'")}
        # ANALYZE's planner statistics are not owned analysis/evidence rows.
        if names - {"sqlite_stat1", "sqlite_stat4"} != TABLES | {"index_generations", "schema_migrations"}:
            raise ValueError("unqualified database table set")
        active = db.execute("SELECT generation_id, repogrammar_version, repository_revision, worktree_hash FROM index_generations WHERE status='active'").fetchall()
        if len(active) != 1:
            raise ValueError("exactly one active generation required")
        generation, *metadata = active[0]
        migrations = db.execute("SELECT version, name FROM schema_migrations ORDER BY version").fetchall()
        result = {"generation_metadata_sha256": hashlib.sha256(canonical_bytes(metadata)).hexdigest(), "schema_migrations_sha256": hashlib.sha256(canonical_bytes(migrations)).hexdigest(), "tables": {}}
        for table in sorted(TABLES):
            columns = [row[1] for row in db.execute(f'PRAGMA table_info("{table}")')]
            if "generation_id" not in columns:
                raise ValueError("generation-scoped table required")
            # Names come from the fixed table allowlist and admitted schema.
            if any(not column.replace("_", "").isalnum() for column in columns):
                raise ValueError("invalid column identifier")
            fields = ", ".join(f'"{column}"' for column in columns)
            row_hashes = []
            for row in db.execute(f'SELECT {fields} FROM "{table}" WHERE generation_id=? ORDER BY {fields}', (generation,)):
                values = []
                for column, value in zip(columns, row):
                    if column == "generation_id":
                        continue
                    if column == "marked_at_generation_id" and value == generation:
                        value = "ACTIVE_GENERATION"
                    if column.endswith("_json"):
                        value = json.loads(value, object_pairs_hook=unique_object)
                    values.append(value)
                encoded = canonical_bytes(values)
                row_hashes.append(hashlib.sha256(encoded).digest())
                if len(row_hashes) > 1_000_000:
                    raise ValueError("table comparison row budget exceeded")
            # SQL ordering of JSON text is not canonical value ordering. Hash
            # a sorted multiset of fixed-size row hashes, retaining duplicates.
            digest = hashlib.sha256()
            for row_hash in sorted(row_hashes):
                digest.update(row_hash)
            result["tables"][table] = {"column_sha256": hashlib.sha256(canonical_bytes([column for column in columns if column != "generation_id"])).hexdigest(), "rows": len(row_hashes), "sha256": digest.hexdigest()}
        return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("left")
    parser.add_argument("right")
    parser.add_argument("--out", required=True)
    args = parser.parse_args()
    left, right = fingerprint(args.left), fingerprint(args.right)
    report = {"schema_version": "full-generation-comparison.v1", "equal": left == right, "left": left, "right": right, "scope": "All admitted active SQLite owned rows; clock/generation identity excluded. Strict full-index A/B only; product/sync-equivalence remain independent gates."}
    Path(args.out).write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    return 0 if report["equal"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
