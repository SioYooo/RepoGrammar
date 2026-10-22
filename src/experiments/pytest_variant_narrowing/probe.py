"""Run the frozen pytest-variant narrowing samples against a product binary.

Read-only against the sources: each snapshot is copied under --out, indexed with
`init --state-only` + `resync`, then queried. No repository code is executed.
Usage: python3 probe.py --bin BINARY --sources DIR --out DIR   (python >= 3.10)
"""

import argparse
import hashlib
import json
import os
import shutil
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
FAMILY_PREFIX = "family:python:pytest_test:framework_pytest_test:"


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


class Product:
    def __init__(self, binary, out):
        self.binary = str(Path(binary).resolve())
        (out / "home").mkdir(parents=True, exist_ok=True)
        self.env = dict(os.environ, HOME=str(out / "home"), REPOGRAMMAR_TELEMETRY="0",
                        DO_NOT_TRACK="1", LC_ALL="C", REPOGRAMMAR_PYTHON_EXECUTABLE=sys.executable)

    def run(self, args, project):
        done = subprocess.run([self.binary, *args, "--project", str(project), "--json"], env=self.env,
                              capture_output=True, text=True, cwd=project.parent, timeout=300)
        try:
            return done.returncode, json.loads(done.stdout)
        except ValueError:
            return done.returncode, {"unparsed": done.stdout[:200], "stderr": done.stderr[:200]}

    def index(self, project):
        for args in (["init", "--state-only"], ["resync", "--progress", "never"]):
            code, _ = self.run(args, project)
            if code:
                raise SystemExit(f"{args[0]} failed for {project}")


def inventory(product, project):
    """family suffix -> (member file paths, complete?)"""
    _, families = product.run(["families"], project)
    result = {}
    for family in families["families"]:
        identifier = family["family_id"]
        if not identifier.startswith(FAMILY_PREFIX):
            continue
        _, detail = product.run(["family", identifier, "--mode", "evidence"], project)
        paths = sorted({m["code_unit_id"].split("#")[0][5:] for m in detail["members"]})
        result[identifier[len(FAMILY_PREFIX):]] = (paths, not detail["members_truncated"])
    return result


def under(path, locus):
    return path == locus or path.startswith(locus.rstrip("/") + "/")


def outcome(code, payload):
    family = payload.get("family") if isinstance(payload.get("family"), dict) else {}
    route = payload.get("query_route") or {}
    selected = family.get("family_id") if payload.get("status") == "ok" else None
    return {
        "exit": code, "status": payload.get("status"), "selected": selected,
        "abstention_reason": (route.get("term_retrieval") or {}).get("abstention_reason"),
        "resolution_cardinality": (payload.get("resolution") or {}).get("cardinality"),
        "candidate_count": len(route.get("candidate_family_ids") or route.get("follow_up_family_ids") or []),
        "read_plan": [[item.get("purpose"), item.get("path")]
                      for item in ((payload.get("read_plan") or {}).get("items") or [])],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bin", type=Path, required=True)
    parser.add_argument("--sources", type=Path, required=True, help="dir with requests/ and fastapi-template/ snapshots")
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    spec = json.loads((HERE / "samples.json").read_text())
    args.out.mkdir(parents=True, exist_ok=True)
    product = Product(args.bin, args.out)
    projects, inventories = {}, {}
    for corpus, info in spec["corpora"].items():
        project = args.out / "proj" / corpus
        shutil.copytree(args.sources / info["snapshot"], project, symlinks=True)
        product.index(project)
        projects[corpus], inventories[corpus] = project, inventory(product, project)
    stale = args.out / "proj" / "stale-fastapi-template"
    shutil.copytree(projects["fastapi-template"], stale, symlinks=True)
    rows = []
    for sample in spec["samples"]:
        target = sample["target"]
        if target.startswith("@"):
            key, _, rest = target[1:].partition(" ")
            target = spec["verbatim"][key][1] + (" " + rest if rest else "")
        project = projects[sample["corpus"]]
        if "mutate" in sample:
            project = stale
            with (stale / sample["mutate"]["path"]).open("a") as handle:
                handle.write(sample["mutate"]["append"])
        code, payload = product.run(["find", target, "--mode", "compact"], project)
        got = outcome(code, payload)
        row = {"id": sample["id"], "class": sample["class"], "target": target, "result": got}
        inv = inventories[sample["corpus"]]
        if "locus" in sample:
            holders = sorted(f for f, (paths, _) in inv.items() if any(under(p, sample["locus"]) for p in paths))
            row["inventory_holders"] = holders
            row["inventory_complete"] = all(done for _, done in inv.values())
        want = sample.get("expect_family_suffix")
        selected = got["selected"] and got["selected"].removeprefix(FAMILY_PREFIX)
        if sample.get("assert") == "none":
            row["pass"] = None
        elif sample["class"] in ("positive", "diagnostic_positive", "control_exact"):
            row["pass"] = selected == want
            if sample["class"] != "control_exact":
                row["premise_ok"] = row["inventory_complete"] and row["inventory_holders"] == [want]
        else:
            row["pass"] = selected is None
            if sample["class"] == "tie" and "locus" in sample:
                row["premise_ok"] = len(row["inventory_holders"]) >= 2 or not row["inventory_complete"]
        rows.append(row)
    summary = {"schema_version": "pytest-variant-narrowing-results.v1",
               "binary_sha256": sha(args.bin), "samples_sha256": sha(HERE / "samples.json"),
               "probe_sha256": sha(__file__),
               "inventory": {c: {f: {"paths": p, "complete": d} for f, (p, d) in i.items()} for c, i in inventories.items()},
               "rows": rows}
    (args.out / "results.json").write_text(json.dumps(summary, indent=1, sort_keys=True))
    for row in rows:
        print(row["id"], row["class"], "PASS" if row["pass"] else ("n/a" if row["pass"] is None else "FAIL"),
              row["result"]["status"], row["result"]["selected"], row["result"]["abstention_reason"],
              row["result"]["resolution_cardinality"], row["result"]["candidate_count"], row.get("premise_ok"))


if __name__ == "__main__":
    main()
