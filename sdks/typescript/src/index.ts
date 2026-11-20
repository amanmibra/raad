/** Native Node.js SDK. Git is required; the raad CLI is not. */
import { createRequire } from "node:module";

interface NativeRepository { call(request: string, token?: string): Promise<string> }
interface Binding { open(options: string): Promise<NativeRepository> }
const binding = createRequire(import.meta.url)("../binding.cjs") as Binding;

export class RaadError extends Error {
  constructor(message: string, public readonly code: string = "unknown", public readonly writeRejected = false) {
    super(message);
    this.name = new.target.name;
  }
}
export class InvalidPathError extends RaadError {}
export class NotFoundError extends RaadError {}
export class CacheMissError extends RaadError {}
export class CacheCorruptionError extends RaadError {}
export class ValidationError extends RaadError {}
export class UnsupportedFeatureError extends ValidationError {}
export class PermissionDeniedError extends RaadError {}
export class ConflictError extends RaadError {}
export class StaleBaseError extends ConflictError {}
export class RemoteUnavailableError extends RaadError {}
export class RateLimitError extends RaadError {}
export class LimitError extends RaadError {}
export class ConfigurationError extends RaadError {}
const errors: Record<string, typeof RaadError> = {
  unsupported_feature: UnsupportedFeatureError, invalid_path: InvalidPathError, not_found: NotFoundError, cache_miss: CacheMissError,
  cache_corruption: CacheCorruptionError, validation: ValidationError,
  permission_denied: PermissionDeniedError, conflict: ConflictError,
  stale_base: StaleBaseError, remote_unavailable: RemoteUnavailableError, rate_limit: RateLimitError,
  limit: LimitError, configuration: ConfigurationError,
};
function nativeError(error: unknown, token?: string): RaadError {
  try {
    const value: unknown = JSON.parse(error instanceof Error ? error.message : "");
    if (typeof value !== "object" || value === null || !("code" in value) || !("message" in value)
        || typeof value.code !== "string" || typeof value.message !== "string") throw new Error();
    const message = token ? value.message.split(token).join("[redacted]") : value.message;
    return new (errors[value.code] ?? RaadError)(message, value.code, "write_rejected" in value && value.write_rejected === true);
  } catch {
    return new RaadError("native raad operation failed");
  }
}

export interface ValidationOptions {
  syntax?: boolean;
  frontmatter?: boolean;
  schemas?: Array<{path: string; schema: unknown}>;
  /** Trusted host commands, not repository-supplied settings. */
  commands?: Array<{name: string; command: string[]; timeoutSeconds?: number}>;
}
export interface Limits {
  fileBytes?: number; mountBytes?: number; maxFiles?: number; responseBytes?: number;
  cacheBytes?: number; retainedRevisions?: number;
}
export type TokenProvider = () => string | Promise<string>;
export interface RepositoryOptions {
  excludedPaths?: string[];
  repository?: string;
  localPath?: string;
  root?: string;
  branch?: string;
  cacheDir?: string;
  refreshSeconds?: number;
  validation?: ValidationOptions;
  limits?: Limits;
  allowProposals?: boolean;
  allowDirect?: boolean;
  anonymous?: boolean;
  tokenProvider?: TokenProvider;
  /** Trusted host API endpoint override for GitHub-compatible test servers. */
  githubApiUrl?: string;
}
export interface Provenance {
  repository: string; branch: string; mountPath: string; commitSha: string;
  filePath: string; stale: boolean;
}
export interface Status {
  activeSha: string | null; branch: string; lastAttempt: number | null;
  lastSync: number | null; lastValidation: number | null; lastError: string | null;
  stale: boolean; retained: string[]; pinned: string[];
}
export interface HistoryEntry { commitSha: string; active: boolean; pinned: boolean }
export interface Proposal {
  baseSha: string; commitSha: string; branch: string;
  pullRequestUrl: string | null; mode: "proposal" | "direct";
}
export type Change = {operation: "put"; path: string; content: string | Uint8Array; executable?: boolean}
  | {operation: "delete"; path: string};
export interface Snapshot { commitSha: string; files: Record<string, Uint8Array>; executables: string[] }
export interface Candidate extends Snapshot { branch: string; rootExists: boolean; issues: string[] }
export interface ProposalStatus { number: number; url: string; state: "open" | "closed" | "merged"; headSha: string; baseSha: string; headBranch: string; baseBranch: string }
export interface ProposalOutcome { outcome: "created" | "updated" | "unchanged"; proposal: Proposal | null; pullRequestNumber: number | null; state: string | null }
interface NativeSnapshot { commit_sha: string; files: Record<string, {content: string; executable: boolean}> }
function snapshot(value: NativeSnapshot): Snapshot {
  return {commitSha: value.commit_sha, files: Object.fromEntries(Object.entries(value.files).map(([p,f]) => [p, Buffer.from(f.content, "base64")])),
    executables: Object.entries(value.files).filter(([,f]) => f.executable).map(([p]) => p)};
}
export interface WriteOptions { expectedBase: string; title: string; body?: string }

interface NativeStatus {
  active_sha: string | null; branch: string; last_attempt: number | null;
  last_sync: number | null; last_validation: number | null; last_error: string | null;
  stale: boolean; retained: string[]; pinned: string[];
}
interface NativeRead {
  data: {content: string; executable: boolean};
  provenance: {repository: string; branch: string; mount_path: string;
    commit_sha: string; file_path: string; stale: boolean};
}
interface NativeProposal {
  base_sha: string; commit_sha: string; branch: string;
  pull_request_url: string | null; mode: "proposal" | "direct";
}
function status(value: NativeStatus): Status {
  return { activeSha: value.active_sha, branch: value.branch, lastAttempt: value.last_attempt,
    lastSync: value.last_sync, lastValidation: value.last_validation, lastError: value.last_error,
    stale: value.stale, retained: value.retained, pinned: value.pinned };
}
function proposal(value: NativeProposal): Proposal {
  return { baseSha: value.base_sha, commitSha: value.commit_sha, branch: value.branch,
    pullRequestUrl: value.pull_request_url, mode: value.mode };
}
export class ReadResult {
  readonly data: Uint8Array;
  readonly executable: boolean;
  readonly provenance: Provenance;
  /** @internal */
  constructor(value: NativeRead) {
    this.data = Buffer.from(value.data.content, "base64");
    this.executable = value.data.executable;
    const p = value.provenance;
    this.provenance = {repository: p.repository, branch: p.branch, mountPath: p.mount_path,
      commitSha: p.commit_sha, filePath: p.file_path, stale: p.stale};
  }
  get commitSha(): string { return this.provenance.commitSha; }
  /** Strict UTF-8 decoding; binary files may throw TypeError. */
  get text(): string { return new TextDecoder("utf-8", {fatal: true}).decode(this.data); }
  json<T = unknown>(): T { return JSON.parse(this.text) as T; }
}

/** A mounted repository. Opening and reads never synchronize implicitly. */
export class Repository {
  private constructor(private readonly native: NativeRepository, private readonly tokenProvider?: TokenProvider, private readonly remote = false) {}

  static async open(options: RepositoryOptions = {}): Promise<Repository> {
    if (options.anonymous && options.tokenProvider) {
      throw new ConfigurationError("choose anonymous access or a token provider", "configuration");
    }
    const validation = options.validation === undefined ? undefined : {
      syntax: options.validation.syntax,
      frontmatter: options.validation.frontmatter,
      schemas: options.validation.schemas,
      commands: options.validation.commands?.map(v => ({name: v.name, command: v.command, timeout_seconds: v.timeoutSeconds})),
    };
    const limits = options.limits === undefined ? undefined : Object.fromEntries(
      Object.entries(options.limits).map(([k, v]) => [k.replace(/[A-Z]/g, c => `_${c.toLowerCase()}`), v]),
    );
    try {
      const native = await binding.open(JSON.stringify({
        excluded_paths: options.excludedPaths, repository: options.repository, local_path: options.localPath, root: options.root,
        branch: options.branch, cache_dir: options.cacheDir, refresh_seconds: options.refreshSeconds,
        validation, limits, allow_proposals: options.allowProposals, allow_direct: options.allowDirect,
        anonymous: options.anonymous, github_api_url: options.githubApiUrl,
      }));
      return new Repository(native, options.tokenProvider, typeof options.repository === "string");
    } catch (error) { throw nativeError(error); }
  }

  private async call<T>(request: Record<string, unknown>): Promise<T> {
    let token: string | undefined;
    if (this.remote && ["sync", "propose", "inspect_candidate", "upsert_proposal", "proposal_status"].includes(String(request.method)) && this.tokenProvider) {
      try {
        token = await this.tokenProvider();
        if (typeof token !== "string" || !token || /[\r\n\0]/.test(token)) throw new Error();
      } catch {
        if (request.method !== "sync") throw new PermissionDeniedError("token provider failed", "permission_denied");
        token = undefined;
        request.token_provider_failed = true;
      }
    }
    try { return JSON.parse(await this.native.call(JSON.stringify(request), token)) as T; }
    catch (error) { throw nativeError(error, token); }
  }

  async sync(revision?: string, options: {pin?: boolean} = {}): Promise<Status> {
    return status(await this.call<NativeStatus>({method: "sync", revision, pin: options.pin ?? false}));
  }
  async snapshot(revision?: string): Promise<Snapshot> {
    return snapshot(await this.call<NativeSnapshot>({method: "snapshot", revision}));
  }
  async inspectCandidate(revision?: string): Promise<Candidate> {
    const value = await this.call<NativeSnapshot & {branch: string; root_exists: boolean; issues: string[]}>({method: "inspect_candidate", revision});
    return {...snapshot(value), branch: value.branch, rootExists: value.root_exists, issues: value.issues};
  }
  async proposalStatus(number: number): Promise<ProposalStatus> {
    const p = await this.call<{number: number; url: string; state: ProposalStatus["state"]; head_sha: string; base_sha: string; head_branch: string; base_branch: string}>({method: "proposal_status", number});
    return {number: p.number, url: p.url, state: p.state, headSha: p.head_sha, baseSha: p.base_sha, headBranch: p.head_branch, baseBranch: p.base_branch};
  }
  async upsertProposal(changes: Change[], options: WriteOptions & {branch: string; operationId: string; expectedHead?: string}): Promise<ProposalOutcome> {
    const request = {expected_base: options.expectedBase, title: options.title, body: options.body ?? "", mode: "proposal", changes: changeEntries(changes)};
    const v = await this.call<{outcome: ProposalOutcome["outcome"]; proposal: NativeProposal | null; pull_request_number: number | null; state: string | null}>({method: "upsert_proposal", request: {request, branch: options.branch, operation_id: options.operationId, expected_head: options.expectedHead ?? null}});
    return {outcome: v.outcome, proposal: v.proposal ? proposal(v.proposal) : null, pullRequestNumber: v.pull_request_number, state: v.state};
  }
  async read(path: string, revision?: string): Promise<ReadResult> {
    return new ReadResult(await this.call<NativeRead>({method: "read", path, revision}));
  }
  list(prefix = "", revision?: string): Promise<string[]> {
    return this.call({method: "list", prefix, revision});
  }
  search(query: string, options: {revision?: string; limit?: number} = {}): Promise<string[]> {
    return this.call({method: "search", query, revision: options.revision, limit: options.limit ?? 100});
  }
  async history(path?: string): Promise<HistoryEntry[]> {
    const result = await this.call<Array<{commit_sha: string; active: boolean; pinned: boolean}>>({method: "history", path});
    return result.map(v => ({commitSha: v.commit_sha, active: v.active, pinned: v.pinned}));
  }
  async status(): Promise<Status> { return status(await this.call<NativeStatus>({method: "status"})); }
  validate(revision?: string): Promise<boolean> { return this.call({method: "validate", revision}); }
  propose(changes: Change[], options: WriteOptions): Promise<Proposal> { return this.write(changes, options, "proposal"); }
  /** Direct write; requires allowDirect: true when opening the repository. */
  commit(changes: Change[], options: WriteOptions): Promise<Proposal> { return this.write(changes, options, "direct"); }
  private async write(changes: Change[], options: WriteOptions, mode: "proposal" | "direct"): Promise<Proposal> {
    const entries = changeEntries(changes);
    return proposal(await this.call<NativeProposal>({method: "propose", request: {
      expected_base: options.expectedBase, title: options.title, body: options.body ?? "", mode, changes: entries,
    }}));
  }
  async pin(sha: string): Promise<void> { await this.call({method: "pin", sha, pinned: true}); }
  async unpin(sha: string): Promise<void> { await this.call({method: "pin", sha, pinned: false}); }
  async prune(): Promise<Status> { return status(await this.call<NativeStatus>({method: "prune"})); }
  async clearCache(): Promise<void> { await this.call({method: "clear_cache"}); }
}

function changeEntries(changes: Change[]) {
  return changes.map(change => {
      if (change.operation === "delete") return {operation: "delete", path: change.path};
      if (change.operation !== "put" || !(typeof change.content === "string" || change.content instanceof Uint8Array)) {
        throw new TypeError("changes must be put/delete objects; put content must be string or Uint8Array");
      }
      return {operation: "put", path: change.path, data: {
        content: Buffer.from(change.content).toString("base64"), executable: change.executable ?? false,
      }};
    });
}
