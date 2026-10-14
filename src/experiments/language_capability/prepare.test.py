import io
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest.mock import patch

import prepare


class SnapshotAdmission(unittest.TestCase):
    def archive(self, root, names, kind=tarfile.REGTYPE):
        archive = root / "snapshot.tar.gz"
        with tarfile.open(archive, "w:gz") as output:
            for name in names:
                entry = tarfile.TarInfo(name)
                entry.type, entry.size, entry.linkname = kind, 3, "outside"
                output.addfile(entry, io.BytesIO(b"abc") if kind == tarfile.REGTYPE else None)
        return archive

    def test_valid_bytes_match_git_identity(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            archive = self.archive(root, ["repo-sha/app.py"])
            files, size = prepare.extract(archive, root / "tree", "repo-sha", {"app.py":prepare.git_blob_sha(b"abc")})
            self.assertEqual((files, size), ({"app.py"}, 3))
            self.assertEqual((root / "tree/app.py").read_bytes(), b"abc")

    def test_traversal_links_duplicates_and_wrong_blob_fail(self):
        cases = [(["repo-sha/../outside"], tarfile.REGTYPE),
                 (["repo-sha/app.py"], tarfile.SYMTYPE),
                 (["repo-sha/app.py"], tarfile.LNKTYPE),
                 (["repo-sha/app.py", "repo-sha/app.py"], tarfile.REGTYPE),
                 (["different/app.py"], tarfile.REGTYPE)]
        for names, kind in cases:
            with self.subTest(names=names, kind=kind), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                with self.assertRaises(ValueError):
                    prepare.extract(self.archive(root, names, kind), root / "tree", "repo-sha", {"app.py":prepare.git_blob_sha(b"abc")})
                self.assertFalse((root / "outside").exists())
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with self.assertRaisesRegex(ValueError, "Git blob"):
                prepare.extract(self.archive(root, ["repo-sha/app.py"]), root / "tree", "repo-sha", {"app.py":"0" * 40})

    def test_exact_size_limit_and_plus_one(self):
        for limit, passes in [(3, True), (2, False)]:
            with self.subTest(limit=limit), tempfile.TemporaryDirectory() as directory, patch.object(prepare, "MAX_FILE", limit):
                root = Path(directory)
                archive = self.archive(root, ["repo-sha/app.py"])
                if passes:
                    self.assertEqual(prepare.extract(archive, root / "tree", "repo-sha", {"app.py":prepare.git_blob_sha(b"abc")})[1], 3)
                else:
                    with self.assertRaises(ValueError):
                        prepare.extract(archive, root / "tree", "repo-sha", {"app.py":prepare.git_blob_sha(b"abc")})

    def test_registered_symlink_blob_is_verified_but_not_materialized(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            archive = self.archive(root, ["repo-sha/link"], tarfile.SYMTYPE)
            expected = {"link":prepare.git_blob_sha(b"outside")}
            self.assertEqual(prepare.extract(archive, root / "tree", "repo-sha", expected, {"link"}), ({"link"}, 0))
            self.assertFalse((root / "tree/link").is_symlink())
            with self.assertRaisesRegex(ValueError, "Git blob"):
                prepare.extract(archive, root / "bad", "repo-sha", {"link":"0" * 40}, {"link"})
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            archive = self.archive(root, ["repo-sha/link"])
            with self.assertRaisesRegex(ValueError, "member type"):
                prepare.extract(archive, root / "tree", "repo-sha", {"link":prepare.git_blob_sha(b"abc")}, {"link"})
            self.assertFalse((root / "tree/link").exists())

    def test_download_rechecks_bytes_after_transport(self):
        with tempfile.TemporaryDirectory() as directory:
            destination = Path(directory) / "download"
            with patch.object(prepare.subprocess, "run", side_effect=lambda *a, **k: destination.write_bytes(b"abcd")):
                with self.assertRaisesRegex(ValueError, "byte bound"):
                    prepare.download("https://example.invalid/snapshot", destination, 3)


if __name__ == "__main__":
    unittest.main()
