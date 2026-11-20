"""Typed results and change requests."""
from __future__ import annotations

import base64
from dataclasses import dataclass
import json
from typing import Any, Sequence


@dataclass(frozen=True)
class Provenance:
    repository: str
    branch: str
    mount_path: str
    commit_sha: str
    file_path: str
    stale: bool


@dataclass(frozen=True)
class ReadResult:
    data: bytes
    provenance: Provenance
    executable: bool

    @property
    def text(self) -> str:
        """Decode UTF-8; raises UnicodeDecodeError for binary files."""
        return self.data.decode("utf-8")

    @property
    def commit_sha(self) -> str:
        return self.provenance.commit_sha

    def json(self) -> Any:
        return json.loads(self.data)

    @classmethod
    def _from_native(cls, result: dict[str, Any]) -> ReadResult:
        return cls(base64.b64decode(result["data"]["content"], validate=True),
                   Provenance(**result["provenance"]), result["data"]["executable"])


@dataclass(frozen=True)
class Status:
    active_sha: str | None
    branch: str
    last_attempt: int | None
    last_sync: int | None
    last_validation: int | None
    last_error: str | None
    stale: bool
    retained: tuple[str, ...]
    pinned: tuple[str, ...]

    @classmethod
    def _from_native(cls, value: dict[str, Any]) -> Status:
        return cls(**{key: value[key] for key in (
            "active_sha", "branch", "last_attempt", "last_sync", "last_validation",
            "last_error", "stale",
        )}, retained=tuple(value["retained"]), pinned=tuple(value["pinned"]))


@dataclass(frozen=True)
class HistoryEntry:
    commit_sha: str
    active: bool
    pinned: bool


@dataclass(frozen=True)
class Proposal:
    base_sha: str
    commit_sha: str
    branch: str
    pull_request_url: str | None
    mode: str


@dataclass(frozen=True)
class Put:
    path: str
    content: str | bytes
    executable: bool = False


@dataclass(frozen=True)
class Delete:
    path: str


def _write_request(changes: Sequence[Put | Delete], expected_base: str,
                   title: str, body: str, mode: str) -> dict[str, Any]:
    result = []
    for change in changes:
        if isinstance(change, Put):
            data = change.content.encode("utf-8") if isinstance(change.content, str) else change.content
            if not isinstance(data, bytes):
                raise TypeError("Put content must be str or bytes")
            result.append({"operation": "put", "path": change.path, "data": {
                "content": base64.b64encode(data).decode("ascii"), "executable": change.executable,
            }})
        elif isinstance(change, Delete):
            result.append({"operation": "delete", "path": change.path})
        else:
            raise TypeError("changes must contain Put or Delete objects")
    return {"expected_base": expected_base, "title": title, "body": body,
            "mode": mode, "changes": result}


@dataclass(frozen=True)
class Snapshot:
    commit_sha: str
    files: dict[str, bytes]
    executables: frozenset[str]

    @classmethod
    def _from_native(cls, value: dict[str, Any]) -> Snapshot:
        return cls(value["commit_sha"], {p: base64.b64decode(f["content"], validate=True) for p, f in value["files"].items()},
                   frozenset(p for p, f in value["files"].items() if f["executable"]))


@dataclass(frozen=True)
class Candidate(Snapshot):
    branch: str
    root_exists: bool
    issues: tuple[str, ...]

    @classmethod
    def _from_native(cls, value: dict[str, Any]) -> Candidate:
        snapshot = Snapshot._from_native(value)
        return cls(snapshot.commit_sha, snapshot.files, snapshot.executables, value["branch"], value["root_exists"], tuple(value["issues"]))


@dataclass(frozen=True)
class ProposalStatus:
    number: int
    url: str
    state: str
    head_sha: str
    base_sha: str
    head_branch: str
    base_branch: str


@dataclass(frozen=True)
class ProposalOutcome:
    outcome: str
    proposal: Proposal | None
    pull_request_number: int | None
    state: str | None

    @classmethod
    def _from_native(cls, value: dict[str, Any]) -> ProposalOutcome:
        return cls(value["outcome"], Proposal(**value["proposal"]) if value["proposal"] else None,
                   value["pull_request_number"], value["state"])
