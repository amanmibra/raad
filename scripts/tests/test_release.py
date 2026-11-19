import importlib.util
import io
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("prepare_release", Path(__file__).resolve().parents[1] / "prepare_release.py")
release = importlib.util.module_from_spec(spec)
spec.loader.exec_module(release)


class CombinedPackageTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)

    def package(self, platform, extra=None):
        path = self.root / f"{platform}.tgz"
        entries = {"package/package.json": b'{"name":"@amanmibra/raad","version":"0.1.0"}',
                   "package/binding.cjs": b"shared platform loader", f"package/raad.{platform}.node": platform.encode()}
        entries.update(extra or {})
        with tarfile.open(path, "w:gz") as archive:
            for name, data in entries.items():
                member = tarfile.TarInfo(name)
                member.size, member.mode = len(data), 0o644
                archive.addfile(member, io.BytesIO(data))
        return path

    def test_retains_both_binaries_and_shared_loader(self):
        paths = [self.package(p) for p in ("linux-x64-gnu", "darwin-arm64")]
        output = self.root / "combined.tgz"
        release.combine_node_packages(paths, output)
        with tarfile.open(output) as archive:
            self.assertEqual(archive.extractfile("package/raad.linux-x64-gnu.node").read(), b"linux-x64-gnu")
            self.assertEqual(archive.extractfile("package/raad.darwin-arm64.node").read(), b"darwin-arm64")
            self.assertEqual(archive.extractfile("package/binding.cjs").read(), b"shared platform loader")

    def test_rejects_mismatched_javascript(self):
        paths = [self.package("linux-x64-gnu"), self.package("darwin-arm64", {"package/binding.cjs": b"different loader"})]
        with self.assertRaisesRegex(ValueError, "different JavaScript"):
            release.combine_node_packages(paths, self.root / "combined.tgz")

    def test_rejects_missing_platform(self):
        with self.assertRaisesRegex(ValueError, "two tested Node binaries"):
            release.combine_node_packages([self.package("linux-x64-gnu")], self.root / "combined.tgz")

    def test_rejects_archive_traversal(self):
        path = self.package("linux-x64-gnu", {"package/../../escaped": b"unsafe"})
        with self.assertRaisesRegex(ValueError, "invalid npm archive path"):
            release.combine_node_packages([path], self.root / "combined.tgz")

    def test_rejects_minor_and_major_release_tags(self):
        for tag in ("v0.2.0", "v1.0.0"):
            with self.subTest(tag=tag), patch("sys.argv", ["prepare_release.py", "--tag", tag]), patch("sys.stderr", new=io.StringIO()):
                with self.assertRaises(SystemExit) as error:
                    release.main()
                self.assertEqual(error.exception.code, 2)


if __name__ == "__main__":
    unittest.main()
