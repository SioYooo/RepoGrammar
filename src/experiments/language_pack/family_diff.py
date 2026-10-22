"""Family-level before/after differential for every committed fixture root.

`snapshot` indexes each fixture root (the roots and the isolated workspace come
from capture.py) and reads the active generation straight from the index: the
code units, the semantic facts per unit, the families with their members, and
the UNKNOWN inventory. Unit ids and content hashes are not kept. `diff`
compares two snapshots and classifies each root into the ADR-0059 slice 3
classes (docs/experiments/tsjs-unit-extraction-tree.md).
"""

import argparse
import json
from pathlib import Path
import shutil
import sqlite3
import subprocess
import sys
import tempfile

sys.path.insert(0, str(Path(__file__).resolve().parent))
from capture import environment, fixture_roots, run

TSJS = ("typescript", "javascript")


def line_of(source, offset):
    return source[:offset].count(b"\n") + 1


def unit_name(unit_id, kind):
    marker = f"#{kind}:"
    body = unit_id.split(marker, 1)[1] if marker in unit_id else unit_id
    return body.rsplit(":", 2)[0] if body.count(":") >= 2 else body


def read_index(project):
    connection = sqlite3.connect(f"file:{project / '.repogrammar/repogrammar.sqlite'}?mode=ro", uri=True)
    try:
        generation, = connection.execute("select generation_id from index_generations where status='active'").fetchone()
        sources = {}

        def source(path):
            if path not in sources:
                sources[path] = (project / path).read_bytes()
            return sources[path]

        units = {}
        for unit_id, path, language, kind, start, end in connection.execute(
                "select code_unit_id, path, language, kind, start_byte, end_byte from code_units where generation_id=?",
                (generation,)):
            units[unit_id] = {"path": path, "language": language, "kind": kind, "start": start, "end": end,
                              "line": line_of(source(path), start), "name": unit_name(unit_id, kind)}
        facts = {}
        for kind, subject, target, assumptions in connection.execute(
                "select kind, subject, target, assumptions_json from semantic_facts where generation_id=?",
                (generation,)):
            facts.setdefault(subject, []).append([kind, target, assumptions])
        for rows in facts.values():
            rows.sort()
        anchor_kind = {}
        for subject, rows in facts.items():
            for _, _, assumptions in rows:
                for item in json.loads(assumptions):
                    if item.startswith("tsjs_anchor_kind="):
                        anchor_kind[subject] = item.split("=", 1)[1]
        unknowns = []
        for subject, rows in facts.items():
            for kind, target, assumptions in rows:
                if kind != "UNKNOWN":
                    continue
                detail = [item.split("=", 1)[1] for item in json.loads(assumptions)
                          if item.startswith("tsjs_unknown_kind=")]
                unit = units.get(subject)
                if unit is None:
                    continue
                unknowns.append([detail[0] if detail else "", target, unit["path"], unit["line"]])
        unknowns.sort()
        families = {}
        for family_id, classification, support in connection.execute(
                "select family_id, classification, supported_member_count from families where generation_id=?",
                (generation,)):
            families[family_id] = {"classification": classification, "support": support, "members": []}
        for family_id, unit_id, role in connection.execute(
                "select family_id, code_unit_id, role from family_members where generation_id=?", (generation,)):
            unit = units[unit_id]
            families[family_id]["members"].append(
                [unit["path"], unit["line"], role, anchor_kind.get(unit_id, ""), unit["kind"]])
        for family in families.values():
            family["members"].sort()
        return {
            "units": sorted([u["path"], u["kind"], u["name"], u["start"], u["end"], u["line"],
                             facts.get(uid, [])] for uid, u in units.items()
                            if u["language"] in TSJS),
            "families": families,
            "unknowns": unknowns,
        }
    finally:
        connection.close()


def snapshot_root(binary, python, worker, root, scratch):
    workspace = Path(tempfile.mkdtemp(prefix="family-diff-", dir=scratch))
    try:
        home = workspace / "home"
        home.mkdir()
        project = workspace / "project"
        shutil.copytree(root, project)
        env = environment(home, python, worker)
        for arguments in (["init", "--yes", "--state-only", "--json", "--progress", "never"],
                          ["resync", "--json", "--progress", "never"]):
            run(binary, env, project, arguments)
        return read_index(project)
    finally:
        shutil.rmtree(workspace)


def match_units(before, after):
    """Pairs units by (path, kind, name, start); returns matched pairs and the rest."""
    index = {}
    for unit in after:
        index.setdefault((unit[0], unit[1], unit[2], unit[3]), []).append(unit)
    matched, only_before = [], []
    for unit in before:
        candidates = index.get((unit[0], unit[1], unit[2], unit[3]))
        if candidates:
            matched.append((unit, candidates.pop(0)))
        else:
            only_before.append(unit)
    only_after = [unit for rows in index.values() for unit in rows]
    return matched, only_before, only_after


def record_fact_change(classes, old, now):
    """A difference in the facts of one unit pair: an anchor change, or UNKNOWN on both sides."""
    if old[6] == now[6]:
        return
    old_facts = {tuple(f) for f in old[6]}
    now_facts = {tuple(f) for f in now[6]}
    only_unknown = bool(old[6] and now[6]) and all(f[0] == "UNKNOWN" for f in old[6] + now[6])
    target = "unknown_kind_changed" if only_unknown else "anchor_changed"
    classes[target].append([old[0], old[1], old[2], old[5],
                            sorted(map(list, old_facts - now_facts)), sorted(map(list, now_facts - old_facts))])


def classify_units(before, after):
    """Unit-level classes: boundary, retired, added, start_corrected, split, merged."""
    matched, gone, new = match_units(before, after)
    classes = {"unit_boundary_only": [], "unit_start_corrected": [], "unit_split": [], "unit_merged": [],
               "unit_retired": [], "unit_added": [], "anchor_changed": [], "unknown_kind_changed": [],
               "unit_kind_changed": []}
    for old, now in matched:
        if old[4] != now[4]:
            classes["unit_boundary_only"].append([old[0], old[1], old[2], old[5]])
        record_fact_change(classes, old, now)
    gone, new = list(gone), list(new)
    for old in list(gone):
        renamed = [u for u in new if u[0] == old[0] and u[1] != old[1] and u[2] == old[2] and u[3] == old[3]]
        if renamed:
            classes["unit_kind_changed"].append([old[0], old[1], old[2], old[5], renamed[0][1]])
            record_fact_change(classes, old, renamed[0])
            gone.remove(old)
            new.remove(renamed[0])
    for old in list(gone):
        # the same unit started elsewhere: same kind and name, overlapping ranges
        containing = [u for u in new if u[0] == old[0] and u[1] == old[1] and u[2] == old[2]
                      and u[3] < old[4] and old[3] < u[4]]
        if containing:
            classes["unit_start_corrected"].append([old[0], old[1], old[2], old[5]])
            record_fact_change(classes, old, containing[0])
            gone.remove(old)
            new.remove(containing[0])
    for old in list(gone):
        inside = [u for u in new if u[0] == old[0] and u[1] == old[1] and old[3] <= u[3] < old[4]]
        if len(inside) >= 2:
            classes["unit_split"].append([old[0], old[1], old[2], old[5]])
            gone.remove(old)
            for unit in inside:
                new.remove(unit)
    for now in list(new):
        inside = [u for u in gone if u[0] == now[0] and u[1] == now[1] and now[3] <= u[3] < now[4]]
        if len(inside) >= 2:
            classes["unit_merged"].append([now[0], now[1], now[2], now[5]])
            new.remove(now)
            for unit in inside:
                gone.remove(unit)
    classes["unit_retired"] = [[u[0], u[1], u[2], u[5], bool(u[6])] for u in gone]
    classes["unit_added"] = [[u[0], u[1], u[2], u[5], bool(u[6])] for u in new]
    return classes


def claims(families):
    rows = set()
    for family_id, family in families.items():
        for path, line, role, anchor, _ in family["members"]:
            rows.add((family_id, path, line, role, anchor, family["support"]))
    return rows


def classify_families(before, after, retired):
    """`family_changed` rows and whether the loss is attributable to retired units."""
    old, new = claims(before["families"]), claims(after["families"])
    if old == new:
        return {"changed": False, "attributable": True, "only_before": [], "only_after": []}
    retired_keys = {(u[0], u[3]) for u in retired}
    # members present before that no unit of the retired set explains stay unexplained
    members_old = {(f, p, l, r, a) for f, p, l, r, a, _ in old}
    members_new = {(f, p, l, r, a) for f, p, l, r, a, _ in new}
    lost = members_old - members_new
    gained = members_new - members_old
    explained = all((p, l) in retired_keys for _, p, l, _, _ in lost)
    lost_families = {f for f, *_ in lost}
    support_ok = True
    for family_id, family in after["families"].items():
        was = before["families"].get(family_id)
        if was and family["support"] != was["support"] and family_id not in lost_families:
            support_ok = False
    return {"changed": True, "attributable": explained and not gained and support_ok,
            "only_before": sorted(map(list, old - new)), "only_after": sorted(map(list, new - old))}


def diff_root(before, after):
    units = classify_units(before["units"], after["units"])
    families = classify_families(before, after, units["unit_retired"])
    unknowns_before = {tuple(u) for u in before["unknowns"]}
    unknowns_after = {tuple(u) for u in after["unknowns"]}
    return {"units": units, "families": families,
            "unknowns": {"only_before": sorted(map(list, unknowns_before - unknowns_after)),
                         "only_after": sorted(map(list, unknowns_after - unknowns_before))}}


def root_label(result):
    """The strongest class of a root, weakest meaning identical."""
    units = result["units"]
    if units["anchor_changed"]:
        return "anchor_changed"
    if result["families"]["changed"]:
        return "family_changed" if not result["families"]["attributable"] else "unit_retired"
    if units["unknown_kind_changed"] or result["unknowns"]["only_before"] or result["unknowns"]["only_after"]:
        return "unknown_kind_changed"
    for name in ("unit_start_corrected", "unit_kind_changed", "unit_split", "unit_merged", "unit_added",
                 "unit_retired", "unit_boundary_only"):
        if units[name]:
            return name
    return "identical"


def snapshot(args):
    repo = args.repo.resolve()
    worker = repo / "src/workers/python/worker.py"
    roots = {}
    for root in fixture_roots(repo / "src/fixtures"):
        roots[str(root.relative_to(repo))] = snapshot_root(args.bin.resolve(), args.python, worker, root,
                                                          args.scratch.resolve())
    args.out.write_text(json.dumps({"schema_version": "family-snapshot.v1", "roots": roots}, sort_keys=True) + "\n")
    print(json.dumps({"roots": len(roots)}))
    return 0


def diff(args):
    before = json.loads(args.before.read_text())["roots"]
    after = json.loads(args.after.read_text())["roots"]
    report, counts = {}, {}
    for root in sorted(before.keys() | after.keys()):
        result = diff_root(before[root], after[root])
        label = root_label(result)
        counts[label] = counts.get(label, 0) + 1
        if label != "identical":
            report[root] = {"label": label, **result}
    summary = {"schema_version": "family-diff.v1", "roots": len(before.keys() | after.keys()), "labels": counts,
               "roots_changed": report}
    args.out.write_text(json.dumps(summary, indent=1, sort_keys=True) + "\n")
    print(json.dumps({"roots": summary["roots"], "labels": counts}, sort_keys=True))
    return 1 if counts.get("anchor_changed") or counts.get("family_changed") else 0


def main(argv):
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    shot = commands.add_parser("snapshot")
    shot.add_argument("--repo", type=Path, required=True)
    shot.add_argument("--bin", type=Path, required=True)
    shot.add_argument("--python", type=Path, required=True)
    shot.add_argument("--scratch", type=Path, required=True)
    shot.add_argument("--out", type=Path, required=True)
    compare = commands.add_parser("diff")
    compare.add_argument("--before", type=Path, required=True)
    compare.add_argument("--after", type=Path, required=True)
    compare.add_argument("--out", type=Path, required=True)
    args = parser.parse_args(argv)
    return snapshot(args) if args.command == "snapshot" else diff(args)


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
