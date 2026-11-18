import asyncio
from collections import deque
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import threading
import unittest

from raad import (
    AsyncRepository, CacheMissError, ConflictError, Delete, InvalidPathError,
    PermissionDeniedError, Put, RateLimitError, Repository, ValidationError,
)


def git(path, *args):
    return subprocess.check_output(["git", "-C", str(path), *args], stderr=subprocess.DEVNULL).decode().strip()


class Fixture:
    def __init__(self):
        self.temp = tempfile.TemporaryDirectory(prefix="raad sdk ")
        self.base = Path(self.temp.name)
        self.repo = self.base / "source"
        self.cache = self.base / "cache"
        self.repo.mkdir()
        git(self.repo, "init", "-b", "main")
        git(self.repo, "config", "user.name", "SDK tests")
        git(self.repo, "config", "user.email", "sdk@example.invalid")
        git(self.repo, "config", "commit.gpgsign", "false")
        git(self.repo, "config", "uploadpack.allowFilter", "true")
        (self.repo / "data").mkdir()
        (self.repo / "data/settings.json").write_text('{"enabled":true}')
        (self.repo / "data/policy.md").write_text("Refund within 30 days.\n")
        (self.repo / "data/binary.bin").write_bytes(b"\x00\xff\x80")
        (self.repo / "outside.txt").write_text("not mounted")
        self.commit()

    def commit(self):
        git(self.repo, "add", ".")
        git(self.repo, "commit", "-qm", "fixture")
        return git(self.repo, "rev-parse", "HEAD")

    def local(self, **options):
        return Repository(local_path=self.repo, root="data", cache_dir=self.cache, **options)

    def remote(self, **options):
        repo = Repository("fixture/private", root="data", branch="main", cache_dir=self.cache, **options)
        directory = next(self.cache.iterdir()) / "objects.git"
        if not directory.exists():
            directory.mkdir()
            git(directory, "init", "--bare")
            git(directory, "remote", "add", "origin", str(self.repo))
        return repo


class Mock:
    def __init__(self, replies):
        self.replies = deque(replies)
        self.requests = []
        outer = self

        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *args):
                pass

            def do_GET(self):
                self.respond()

            def do_POST(self):
                self.respond()

            def respond(self):
                body = self.rfile.read(int(self.headers.get("Content-Length", "0")))
                outer.requests.append((self.command, self.path, self.headers.get("Authorization"), json.loads(body) if body else None))
                code, value = outer.replies.popleft() if outer.replies else (500, {})
                self.send_response(code)
                self.send_header("Content-Type", "application/json")
                if code == 429:
                    self.send_header("Retry-After", "60")
                self.end_headers()
                self.wfile.write(json.dumps(value).encode())

        self.server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.url = f"http://127.0.0.1:{self.server.server_port}/"
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()

    def close(self):
        self.server.shutdown()
        self.server.server_close()
        self.thread.join()


def replies(base):
    return [(200, {"object": {"sha": base}}), (200, {"tree": {"sha": "base-tree"}}),
            (201, {"sha": "blob"}), (201, {"sha": "tree"}), (201, {"sha": "a" * 40}),
            (200, {"object": {"sha": base}}), (201, {"ref": "proposal"}),
            (201, {"html_url": "https://github.com/fixture/private/pull/1"})]


class RepositoryTests(unittest.TestCase):
    def setUp(self):
        self.f = Fixture()
        self.addCleanup(self.f.temp.cleanup)

    def test_reads_are_explicit_scoped_and_binary_safe(self):
        repo = self.f.local()
        with self.assertRaises(CacheMissError):
            repo.read("settings.json")
        sha = repo.sync().active_sha
        file = repo.read("settings.json")
        self.assertEqual(file.json(), {"enabled": True})
        self.assertEqual(file.commit_sha, sha)
        self.assertEqual(file.provenance.mount_path, "data")
        self.assertEqual(repo.read("binary.bin").data, b"\x00\xff\x80")
        with self.assertRaises(UnicodeDecodeError):
            _ = repo.read("binary.bin").text
        self.assertEqual(repo.search("30 days"), ["policy.md"])
        self.assertEqual(len(repo.list()), 3)
        self.assertTrue(repo.validate())
        with self.assertRaises(InvalidPathError):
            repo.read("../outside.txt")
        with self.assertRaises(InvalidPathError):
            repo.read("policy.md", "HEAD")

    def test_pins_and_validation_rejection_survive_reopening(self):
        repo = self.f.local()
        first = repo.sync().active_sha
        (self.f.repo / "data/settings.json").write_text('{"enabled":false}')
        second = self.f.commit()
        self.assertEqual(repo.read("settings.json").commit_sha, first)
        repo.sync()
        self.assertEqual(repo.read("settings.json", first).json(), {"enabled": True})
        repo.pin(first)
        self.assertIn(first, repo.status().pinned)
        repo.unpin(first)
        self.assertEqual(len(repo.history()), 2)
        self.assertEqual(repo.prune().active_sha, second)
        (self.f.repo / "data/settings.json").write_text("bad JSON")
        self.f.commit()
        with self.assertRaises(ValidationError):
            repo.sync()
        self.assertEqual(self.f.local().read("settings.json").commit_sha, second)
        self.assertTrue(repo.status().stale)
        repo.clear_cache()
        with self.assertRaises(CacheMissError):
            repo.read("settings.json")

    def test_warm_offline_and_token_callback_only_on_remote_operations(self):
        calls = []
        repo = self.f.remote(token_provider=lambda: calls.append(1) or "test-secret")
        first = repo.sync().active_sha
        self.assertEqual(len(calls), 1)
        repo.read("policy.md")
        repo.list()
        repo.status()
        self.assertEqual(len(calls), 1)
        objects = next(self.f.cache.iterdir()) / "objects.git"
        git(objects, "remote", "set-url", "origin", str(self.f.base / "missing"))
        with self.assertRaises(Exception):
            repo.sync()
        self.assertEqual(repo.read("policy.md").commit_sha, first)
        self.assertTrue(repo.status().stale)
        self.assertEqual(len(calls), 2)

    def test_readonly_and_token_provider_failure(self):
        repo = self.f.remote()
        sha = repo.sync().active_sha
        with self.assertRaises(PermissionDeniedError):
            repo.propose([Put("policy.md", "updated")], expected_base=sha, title="Change")
        with self.assertRaises(PermissionDeniedError):
            repo.commit([Delete("policy.md")], expected_base=sha, title="Change")

        def broken():
            raise RuntimeError("secret-token-must-not-leak")

        repo = self.f.remote(token_provider=broken)
        with self.assertRaises(PermissionDeniedError) as error:
            repo.sync()
        self.assertNotIn("secret-token", str(error.exception))
        self.assertIsNone(error.exception.__cause__)

    def test_authenticated_multi_file_proposal_and_direct_commit(self):
        base = git(self.f.repo, "rev-parse", "HEAD")
        mock = Mock(replies(base))
        self.addCleanup(mock.close)
        repo = self.f.remote(allow_proposals=True, allow_direct=True,
                             token_provider=lambda: "test-secret", github_api_url=mock.url)
        repo.sync()
        result = repo.propose([Put("policy.md", "new"), Delete("settings.json")],
                              expected_base=base, title="Update policy")
        self.assertEqual(result.pull_request_url, "https://github.com/fixture/private/pull/1")
        self.assertEqual(repo.status().active_sha, base)
        self.assertEqual(len(mock.requests), 8)
        self.assertTrue(all(req[2] == "Bearer test-secret" for req in mock.requests))
        self.assertTrue(all(entry["path"].startswith("data/") for entry in mock.requests[3][3]["tree"]))
        mock.replies.extend([(200, {"object": {"sha": base}}),
                             (200, {"data": {"createCommitOnBranch": {"commit": {"oid": "b" * 40}}}})])
        result = repo.commit([Put("policy.md", b"\x00\xff")], expected_base=base, title="Direct")
        self.assertIsNone(result.pull_request_url)
        self.assertEqual(mock.requests[-1][3]["variables"]["input"]["expectedHeadOid"], base)

    def test_stale_base_rate_limit_and_error_redaction(self):
        mock = Mock([(200, {"object": {"sha": "b" * 40}}),
                     (429, {"message": "test-secret"}), (403, {"message": "test-secret"})])
        self.addCleanup(mock.close)
        repo = self.f.remote(allow_proposals=True, token_provider=lambda: "test-secret", github_api_url=mock.url)
        base = repo.sync().active_sha
        for exception in (ConflictError, RateLimitError, PermissionDeniedError):
            with self.assertRaises(exception) as error:
                repo.propose([Put("policy.md", "new")], expected_base=base, title="Change")
            self.assertNotIn("test-secret", str(error.exception))
        self.assertEqual(len(mock.requests), 3)

    def test_schema_validation_and_symlink_rejection(self):
        repo = self.f.local(validation={"schemas": [{"path": "settings.json", "schema": {
            "type": "object", "required": ["missing"],
        }}]})
        with self.assertRaises(ValidationError):
            repo.sync()
        if sys.platform != "win32":
            (self.f.repo / "data/link").symlink_to("../outside.txt")
            self.f.commit()
            with self.assertRaises(ValidationError):
                self.f.local().sync()


class AsyncTests(unittest.IsolatedAsyncioTestCase):
    async def test_native_sync_and_reads_do_not_block_event_loop(self):
        f = await asyncio.to_thread(Fixture)
        self.addCleanup(f.temp.cleanup)
        ticks = 0

        async def heartbeat():
            nonlocal ticks
            while True:
                ticks += 1
                await asyncio.sleep(0.01)

        repo = await AsyncRepository.open(local_path=f.repo, root="data", cache_dir=f.cache,
            validation={"commands": [{"name": "slow", "command": [sys.executable, "-c", "import time; time.sleep(.2)"]}]})
        task = asyncio.create_task(heartbeat())
        try:
            await repo.sync()
            self.assertGreater(ticks, 5)
            result = await repo.read("settings.json")
            self.assertEqual(result.json(), {"enabled": True})
            await asyncio.gather(repo.sync(), repo.sync())
            self.assertEqual(len(await repo.history()), 1)
        finally:
            task.cancel()
            await asyncio.gather(task, return_exceptions=True)

    async def test_async_token_provider_refresh_and_cached_reads(self):
        f = await asyncio.to_thread(Fixture)
        self.addCleanup(f.temp.cleanup)
        calls = []

        async def provider():
            await asyncio.sleep(0)
            calls.append(1)
            return "async-secret"

        # Seed the filtered local test remote using the synchronous adapter.
        await asyncio.to_thread(f.remote)
        repo = await AsyncRepository.open("fixture/private", root="data", branch="main",
                                           cache_dir=f.cache, token_provider=provider)
        await repo.sync()
        await repo.read("policy.md")
        self.assertEqual(len(calls), 1)
        await repo.sync()
        self.assertEqual(len(calls), 2)


if __name__ == "__main__":
    unittest.main()
