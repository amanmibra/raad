"""Python interfaces to the in-process Rust core. Reads never synchronize."""
from __future__ import annotations

import asyncio
import inspect
import json
import os
from typing import Any, Awaitable, Callable, Sequence

from . import _native
from .errors import PermissionDeniedError, native_error
from .models import Candidate, Snapshot, ProposalOutcome, ProposalStatus, Delete, HistoryEntry, Proposal, Put, ReadResult, Status, _write_request

TokenProvider = Callable[[], str]
AsyncTokenProvider = Callable[[], str | Awaitable[str]]


def _check_token(token: Any) -> str:
    if not isinstance(token, str) or not token or any(c in token for c in "\r\n\0"):
        raise PermissionDeniedError("token provider must return a nonempty token")
    return token


class Repository:
    """A mounted repository with explicit sync, persistent cache, and scoped writes.

    Use repository='owner/repo' for GitHub, or local_path for an existing local
    checkout. Construction and reads do not access GitHub. Write permissions
    default to false. Validation settings are trusted host configuration.
    """

    def __init__(
        self, repository: str | None = None, *, local_path: str | os.PathLike[str] | None = None,
        root: str = "", branch: str | None = None,
        cache_dir: str | os.PathLike[str] | None = None, refresh_seconds: int = 60,
        validation: dict[str, Any] | None = None, limits: dict[str, Any] | None = None,
        allow_proposals: bool = False, allow_direct: bool = False,
        anonymous: bool = False, token_provider: TokenProvider | None = None,
        github_api_url: str | None = None, excluded_paths: Sequence[str] = (),
    ):
        if anonymous and token_provider is not None:
            raise ValueError("choose anonymous access or a token provider")
        options: dict[str, Any] = {
            "excluded_paths": list(excluded_paths), "repository": repository, "local_path": os.fspath(local_path) if local_path is not None else None,
            "root": root, "branch": branch, "cache_dir": os.fspath(cache_dir) if cache_dir is not None else None,
            "refresh_seconds": refresh_seconds, "allow_proposals": allow_proposals,
            "allow_direct": allow_direct, "anonymous": anonymous, "github_api_url": github_api_url,
        }
        if validation is not None:
            options["validation"] = validation
        if limits is not None:
            options["limits"] = limits
        try:
            self._native = _native.Repository(json.dumps(options))
        except RuntimeError as error:
            raise native_error(error) from None
        self._token_provider = token_provider
        self._remote = repository is not None

    def _call_native(self, request: dict[str, Any], token: str | None) -> Any:
        try:
            return json.loads(self._native.call(json.dumps(request), token))
        except RuntimeError as error:
            raise native_error(error, token) from None

    def _call(self, request: dict[str, Any]) -> Any:
        token = None
        if self._remote and request["method"] in ("sync", "propose", "inspect_candidate", "upsert_proposal", "proposal_status") and self._token_provider is not None:
            try:
                token = _check_token(self._token_provider())
            except Exception:
                if request["method"] != "sync":
                    raise PermissionDeniedError("token provider failed") from None
                request["token_provider_failed"] = True
        return self._call_native(request, token)

    def sync(self, revision: str | None = None, *, pin: bool = False) -> Status:
        return Status._from_native(self._call({"method": "sync", "revision": revision, "pin": pin}))

    def snapshot(self, revision: str | None = None) -> Snapshot:
        return Snapshot._from_native(self._call({"method": "snapshot", "revision": revision}))

    def inspect_candidate(self, revision: str | None = None) -> Candidate:
        return Candidate._from_native(self._call({"method": "inspect_candidate", "revision": revision}))

    def proposal_status(self, number: int) -> ProposalStatus:
        return ProposalStatus(**self._call({"method": "proposal_status", "number": number}))

    def upsert_proposal(self, changes: Sequence[Put | Delete], *, expected_base: str,
                              title: str, branch: str, operation_id: str,
                              expected_head: str | None = None, body: str = "") -> ProposalOutcome:
        request = _write_request(changes, expected_base, title, body, "proposal")
        return ProposalOutcome._from_native(self._call({"method": "upsert_proposal", "request": {
            "request": request, "branch": branch, "expected_head": expected_head, "operation_id": operation_id,
        }}))

    def read(self, path: str, revision: str | None = None) -> ReadResult:
        return ReadResult._from_native(self._call({"method": "read", "path": path, "revision": revision}))

    def list(self, prefix: str = "", revision: str | None = None) -> list[str]:
        return self._call({"method": "list", "prefix": prefix, "revision": revision})

    def search(self, query: str, revision: str | None = None, *, limit: int = 100) -> list[str]:
        return self._call({"method": "search", "query": query, "revision": revision, "limit": limit})

    def history(self, path: str | None = None) -> list[HistoryEntry]:
        return [HistoryEntry(**value) for value in self._call({"method": "history", "path": path})]

    def status(self) -> Status:
        return Status._from_native(self._call({"method": "status"}))

    def validate(self, revision: str | None = None) -> bool:
        return self._call({"method": "validate", "revision": revision})

    def propose(self, changes: Sequence[Put | Delete], *, expected_base: str,
                title: str, body: str = "") -> Proposal:
        request = _write_request(changes, expected_base, title, body, "proposal")
        return Proposal(**self._call({"method": "propose", "request": request}))

    def commit(self, changes: Sequence[Put | Delete], *, expected_base: str,
               title: str, body: str = "") -> Proposal:
        """Direct write; requires allow_direct=True at construction."""
        request = _write_request(changes, expected_base, title, body, "direct")
        return Proposal(**self._call({"method": "propose", "request": request}))

    def pin(self, sha: str) -> None:
        self._call({"method": "pin", "sha": sha, "pinned": True})

    def unpin(self, sha: str) -> None:
        self._call({"method": "pin", "sha": sha, "pinned": False})

    def prune(self) -> Status:
        return Status._from_native(self._call({"method": "prune"}))

    def clear_cache(self) -> None:
        self._call({"method": "clear_cache"})


class AsyncRepository:
    """Async interface for FastAPI and other asyncio hosts.

    Native operations run in worker threads; token providers may be sync or async.
    """

    def __init__(self, repository: Repository, token_provider: AsyncTokenProvider | None):
        self._repository = repository
        self._token_provider = token_provider

    @classmethod
    async def open(cls, repository: str | None = None, *,
                   token_provider: AsyncTokenProvider | None = None, **options: Any) -> AsyncRepository:
        if options.get("anonymous") and token_provider is not None:
            raise ValueError("choose anonymous access or a token provider")
        instance = await asyncio.to_thread(Repository, repository, **options)
        return cls(instance, token_provider)

    async def _call(self, request: dict[str, Any]) -> Any:
        token = None
        if self._repository._remote and request["method"] in ("sync", "propose", "inspect_candidate", "upsert_proposal", "proposal_status") and self._token_provider is not None:
            try:
                # Blocking providers also run off the host's event loop.
                value = await asyncio.to_thread(self._token_provider)
                token = _check_token(await value if inspect.isawaitable(value) else value)
            except Exception:
                if request["method"] != "sync":
                    raise PermissionDeniedError("token provider failed") from None
                request["token_provider_failed"] = True
        return await asyncio.to_thread(self._repository._call_native, request, token)

    async def sync(self, revision: str | None = None, *, pin: bool = False) -> Status:
        return Status._from_native(await self._call({"method": "sync", "revision": revision, "pin": pin}))

    async def snapshot(self, revision: str | None = None) -> Snapshot:
        return Snapshot._from_native(await self._call({"method": "snapshot", "revision": revision}))

    async def inspect_candidate(self, revision: str | None = None) -> Candidate:
        return Candidate._from_native(await self._call({"method": "inspect_candidate", "revision": revision}))

    async def proposal_status(self, number: int) -> ProposalStatus:
        return ProposalStatus(**await self._call({"method": "proposal_status", "number": number}))

    async def upsert_proposal(self, changes: Sequence[Put | Delete], *, expected_base: str,
                              title: str, branch: str, operation_id: str,
                              expected_head: str | None = None, body: str = "") -> ProposalOutcome:
        request = _write_request(changes, expected_base, title, body, "proposal")
        return ProposalOutcome._from_native(await self._call({"method": "upsert_proposal", "request": {
            "request": request, "branch": branch, "expected_head": expected_head, "operation_id": operation_id,
        }}))

    async def read(self, path: str, revision: str | None = None) -> ReadResult:
        return ReadResult._from_native(await self._call({"method": "read", "path": path, "revision": revision}))

    async def list(self, prefix: str = "", revision: str | None = None) -> list[str]:
        return await self._call({"method": "list", "prefix": prefix, "revision": revision})

    async def search(self, query: str, revision: str | None = None, *, limit: int = 100) -> list[str]:
        return await self._call({"method": "search", "query": query, "revision": revision, "limit": limit})

    async def history(self, path: str | None = None) -> list[HistoryEntry]:
        return [HistoryEntry(**value) for value in await self._call({"method": "history", "path": path})]

    async def status(self) -> Status:
        return Status._from_native(await self._call({"method": "status"}))

    async def validate(self, revision: str | None = None) -> bool:
        return await self._call({"method": "validate", "revision": revision})

    async def propose(self, changes: Sequence[Put | Delete], *, expected_base: str,
                      title: str, body: str = "") -> Proposal:
        request = _write_request(changes, expected_base, title, body, "proposal")
        return Proposal(**await self._call({"method": "propose", "request": request}))

    async def commit(self, changes: Sequence[Put | Delete], *, expected_base: str,
                     title: str, body: str = "") -> Proposal:
        request = _write_request(changes, expected_base, title, body, "direct")
        return Proposal(**await self._call({"method": "propose", "request": request}))

    async def pin(self, sha: str) -> None:
        await self._call({"method": "pin", "sha": sha, "pinned": True})

    async def unpin(self, sha: str) -> None:
        await self._call({"method": "pin", "sha": sha, "pinned": False})

    async def prune(self) -> Status:
        return Status._from_native(await self._call({"method": "prune"}))

    async def clear_cache(self) -> None:
        await self._call({"method": "clear_cache"})
