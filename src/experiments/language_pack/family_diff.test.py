from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))
from family_diff import classify_units, diff_root, root_label, unit_name


def unit(kind, name, start, end, line=1, facts=None, path="a.ts"):
    return [path, kind, name, start, end, line, facts or []]


def root(units, families=None, unknowns=None):
    return {"units": units, "families": families or {}, "unknowns": unknowns or []}


def family(members, support):
    return {"classification": "SUPPORTED_PATTERN", "support": support,
            "members": [[p, l, "framework:x", "x", "express_route"] for p, l in members]}


class FamilyDiffTest(unittest.TestCase):
    def test_unit_name_reads_the_slug_between_kind_and_range(self):
        self.assertEqual(unit_name("unit:a.ts#express_route:get:10-20:3", "express_route"), "get")

    def test_end_only_difference_is_boundary_only(self):
        result = diff_root(root([unit("function", "f", 0, 10)]), root([unit("function", "f", 0, 12)]))
        self.assertEqual(root_label(result), "unit_boundary_only")

    def test_retired_unit_with_its_family_member_is_attributable(self):
        old = root([unit("test_case", "it", 5, 9, 2, [["SYMBOL", "t", "[]"]]), unit("test_case", "it", 20, 30, 3)],
                   {"fam": family([("a.ts", 2), ("a.ts", 3)], 2)})
        new = root([unit("test_case", "it", 20, 31, 3)], {"fam": family([("a.ts", 3)], 1)})
        result = diff_root(old, new)
        self.assertTrue(result["families"]["changed"])
        self.assertTrue(result["families"]["attributable"])
        self.assertEqual(root_label(result), "unit_retired")

    def test_added_family_member_is_not_attributable(self):
        old = root([unit("test_case", "it", 5, 9, 2)], {"fam": family([("a.ts", 2)], 1)})
        new = root([unit("test_case", "it", 5, 9, 2), unit("test_case", "it", 20, 30, 3)],
                   {"fam": family([("a.ts", 2), ("a.ts", 3)], 2)})
        self.assertEqual(root_label(diff_root(old, new)), "family_changed")

    def test_anchor_change_and_unknown_kind_change_are_distinguished(self):
        anchored = [["SYMBOL", "express.get", "[\"a\"]"]]
        other = [["SYMBOL", "express.get", "[\"b\"]"]]
        self.assertEqual(
            root_label(diff_root(root([unit("express_route", "get", 0, 5, facts=anchored)]),
                                 root([unit("express_route", "get", 0, 5, facts=other)]))),
            "anchor_changed")
        before = [["UNKNOWN", "FrameworkMagic", "[\"k1\"]"]]
        after = [["UNKNOWN", "UnresolvedImport", "[\"k2\"]"]]
        self.assertEqual(
            root_label(diff_root(root([unit("express_route", "get", 0, 5, facts=before)]),
                                 root([unit("express_route", "get", 0, 5, facts=after)]))),
            "unknown_kind_changed")

    def test_mid_chain_unit_is_start_corrected_and_orphans_are_retired_or_added(self):
        classes = classify_units([unit("drizzle_query", "q", 10, 40)], [unit("drizzle_query", "q", 4, 40)])
        self.assertEqual(len(classes["unit_start_corrected"]), 1)
        classes = classify_units([unit("function", "f", 0, 5)], [unit("function", "g", 0, 5)])
        self.assertEqual((len(classes["unit_retired"]), len(classes["unit_added"])), (1, 1))

    def test_split_and_merge(self):
        split = classify_units([unit("function", "f", 0, 50)],
                               [unit("function", "a", 1, 10), unit("function", "b", 20, 30)])
        self.assertEqual(len(split["unit_split"]), 1)
        merged = classify_units([unit("function", "a", 1, 10), unit("function", "b", 20, 30)],
                                [unit("function", "f", 0, 50)])
        self.assertEqual(len(merged["unit_merged"]), 1)


if __name__ == "__main__":
    unittest.main()
