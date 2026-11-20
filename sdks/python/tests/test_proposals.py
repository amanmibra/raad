import unittest
from pathlib import Path

from raad import CacheMissError, ConflictError, InvalidPathError, Put, RemoteUnavailableError, StaleBaseError, ValidationError
from test_sdk import Fixture, git
from github_server import GitHubServer


class ProposalTests(unittest.TestCase):
    def setUp(self):
        self.f = Fixture()
        self.server = GitHubServer(self.f.repo)
        self.repo = self.f.remote(allow_proposals=True, token_provider=lambda: "test-token", github_api_url=self.server.url)
        self.base = self.repo.sync(pin=True).active_sha

    def tearDown(self):
        self.server.close()
        self.f.temp.cleanup()

    def send(self, **options):
        return self.repo.upsert_proposal([Put("settings.json", '{"enabled":false}')], expected_base=self.base,
                                        title="Change settings", branch="application/sync", operation_id="operation-1", **options)

    def test_create_update_recover_and_protect_human_edits(self):
        self.server.fail_pr = 1
        with self.assertRaises(RemoteUnavailableError):
            self.send()
        sha = git(self.f.repo, "rev-parse", "application/sync")
        mutations = sum(m == "POST" for m, _, _ in self.server.requests)
        result = self.send()
        self.assertEqual(result.outcome, "created")
        self.assertEqual(result.proposal.commit_sha, sha)
        self.assertEqual(sum(m == "POST" for m, _, _ in self.server.requests), mutations + 1)
        again = self.send()
        self.assertEqual(again.outcome, "unchanged")
        self.assertEqual(len(self.server.pulls), 1)
        self.assertEqual(self.repo.proposal_status(1).head_sha, sha)
        updated = self.repo.upsert_proposal([Put("settings.json", '{"new":true}')], expected_base=self.base,
                title="New settings", branch="application/sync", operation_id="operation-2", expected_head=sha)
        self.assertEqual(updated.outcome, "updated")
        self.assertEqual(len(self.server.pulls), 1)
        # Advance a human commit on the proposal branch, preserving its machine ancestry.
        human = self.server.git("commit-tree", self.server.git("rev-parse", "application/sync^{tree}"),
                                "-p", updated.proposal.commit_sha, data=b"human edit")
        self.server.git("update-ref", "refs/heads/application/sync", human)
        with self.assertRaises(ConflictError):
            self.repo.upsert_proposal([Put("settings.json", '{}')], expected_base=self.base, title="Next", branch="application/sync",
                                     operation_id="operation-3", expected_head=updated.proposal.commit_sha)
        self.assertEqual(git(self.f.repo, "rev-parse", "application/sync"), human)

    def test_retry_after_old_pr_closes_uses_the_new_operation_pr(self):
        first = self.send()
        self.server.pulls[0]["state"] = "closed"
        self.server.fail_pr = 1
        options = dict(expected_base=self.base, title="Next operation", branch="application/sync",
                       operation_id="next-operation", expected_head=first.proposal.commit_sha)
        changes = [Put("settings.json", '{"next":true}')]
        with self.assertRaises(RemoteUnavailableError):
            self.repo.upsert_proposal(changes, **options)
        recovered = self.repo.upsert_proposal(changes, **options)
        self.assertEqual(recovered.pull_request_number, 2)
        self.assertEqual(recovered.state, "open")
        # A later update belongs to PR 2 even when it closes before retry.
        latest = self.repo.upsert_proposal([Put("settings.json", '{"last":true}')],
                  **{**options, "operation_id": "last-operation", "expected_head": recovered.proposal.commit_sha})
        self.server.pulls[1]["state"] = "closed"
        retried = self.repo.upsert_proposal([Put("settings.json", '{"last":true}')],
                  **{**options, "operation_id": "last-operation", "expected_head": recovered.proposal.commit_sha})
        self.assertEqual(retried.pull_request_number, 2)
        self.assertEqual(retried.state, "closed")
        self.assertEqual(len(self.server.pulls), 2)

    def test_removed_closed_pr_marker_requires_review(self):
        self.send()
        self.server.pulls[0].update(state="closed", body="edited by a person")
        with self.assertRaises(ConflictError):
            self.send()
        self.assertEqual(len(self.server.pulls), 1)

    def test_stale_base_rejection_and_recovery_after_source_advances(self):
        (self.f.repo / "outside.txt").write_text("advanced source")
        self.f.commit()
        with self.assertRaises(StaleBaseError) as rejected:
            self.send()
        self.assertTrue(rejected.exception.write_rejected)
        self.assertFalse(any(method == "POST" for method, _, _ in self.server.requests))
        # A published ref can finish PR creation after the source advances.
        self.base = self.repo.sync(pin=True).active_sha
        self.server.fail_pr = 1
        with self.assertRaises(RemoteUnavailableError):
            self.send()
        (self.f.repo / "outside.txt").write_text("advanced again")
        self.f.commit()
        recovered = self.send()
        self.assertEqual(recovered.state, "open")
        self.assertEqual(recovered.outcome, "created")

    def test_validation_rejection_is_distinct_from_unknown_write_outcome(self):
        with self.assertRaises(ValidationError) as rejected:
            self.repo.upsert_proposal([Put("settings.json", 'invalid')], expected_base=self.base,
                  title="Invalid", branch="application/sync", operation_id="invalid")
        self.assertTrue(rejected.exception.write_rejected)
        self.assertFalse(any(method == "POST" for method, _, _ in self.server.requests))
        self.server.fail_pr = 1
        with self.assertRaises(RemoteUnavailableError) as unknown:
            self.send()
        self.assertFalse(unknown.exception.write_rejected)

    def test_atomic_cas_race_and_noop(self):
        self.server.before_cas = lambda change: self.server.git("update-ref", change["name"], self.base)
        with self.assertRaises(ConflictError):
            self.send()
        self.assertEqual(len(self.server.pulls), 0)
        # No diff has no remote mutations, even on an unused proposal branch.
        self.server.before_cas = None
        count = sum(m == "POST" for m, _, _ in self.server.requests)
        result = self.repo.upsert_proposal([], expected_base=self.base, title="No changes", branch="application/noop", operation_id="noop")
        self.assertEqual(result.outcome, "unchanged")
        self.assertIsNone(result.proposal)
        self.assertEqual(sum(m == "POST" for m, _, _ in self.server.requests), count)

    def test_raw_inspection_repair_missing_and_excluded(self):
        (self.f.repo / "data/build").mkdir()
        (self.f.repo / "data/build/broken.json").write_text("invalid")
        self.f.commit()
        filtered = self.f.local(excluded_paths=["build"])
        filtered.sync(pin=True)
        self.assertNotIn("build/broken.json", filtered.snapshot().files)
        (self.f.repo / "data/settings.json").write_text("invalid")
        broken = self.f.commit()
        candidate = self.repo.inspect_candidate()
        self.assertEqual(candidate.commit_sha, broken)
        self.assertTrue(candidate.issues)
        self.assertEqual(self.repo.status().active_sha, self.base)
        with self.assertRaises(ValidationError):
            self.repo.sync()
        # Repair all invalid files in one proposal from the unactivated raw revision.
        fixed = self.repo.upsert_proposal([Put("settings.json", '{}'), Put("build/broken.json", '{}')], expected_base=broken,
                    title="Repair", branch="application/repair", operation_id="repair")
        self.assertEqual(fixed.outcome, "created")
        from raad import Repository
        repo = Repository(local_path=self.f.repo, root="absent", cache_dir=self.f.cache)
        self.assertFalse(repo.inspect_candidate().root_exists)
        with self.assertRaises(CacheMissError):
            repo.snapshot()


if __name__ == "__main__":
    unittest.main()
