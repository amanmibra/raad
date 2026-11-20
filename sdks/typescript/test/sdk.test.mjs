import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, writeFileSync, readdirSync, existsSync, rmSync, symlinkSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { execFileSync } from 'node:child_process';
import { createServer } from 'node:http';
import test from 'node:test';
import {
  Repository, CacheMissError, ConflictError, InvalidPathError,
  PermissionDeniedError, RateLimitError, ValidationError,
} from '../dist/index.js';

function git(path, ...args) {
  return execFileSync('git', ['-C', path, ...args], {encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe']}).trim();
}
function fixture(t) {
  const base = mkdtempSync(join(tmpdir(), 'raad sdk '));
  t.after(() => rmSync(base, {recursive: true, force: true}));
  const repo = join(base, 'source'), cache = join(base, 'cache');
  mkdirSync(repo); git(repo, 'init', '-b', 'main');
  git(repo, 'config', 'user.name', 'SDK tests'); git(repo, 'config', 'user.email', 'sdk@example.invalid');
  git(repo, 'config', 'commit.gpgsign', 'false'); git(repo, 'config', 'uploadpack.allowFilter', 'true');
  mkdirSync(join(repo, 'data'));
  writeFileSync(join(repo, 'data/settings.json'), '{"enabled":true}');
  writeFileSync(join(repo, 'data/policy.md'), 'Refund within 30 days.\n');
  writeFileSync(join(repo, 'data/binary.bin'), Buffer.from([0, 255, 128]));
  writeFileSync(join(repo, 'outside.txt'), 'not mounted');
  const commit = () => { git(repo, 'add', '.'); git(repo, 'commit', '-qm', 'fixture'); return git(repo, 'rev-parse', 'HEAD'); };
  commit();
  const local = options => Repository.open({localPath: repo, root: 'data', cacheDir: cache, ...options});
  const remote = async options => {
    const instance = await Repository.open({repository: 'fixture/private', root: 'data', branch: 'main', cacheDir: cache, ...options});
    const objects = join(cache, readdirSync(cache)[0], 'objects.git');
    if (!existsSync(objects)) { mkdirSync(objects); git(objects, 'init', '--bare'); git(objects, 'remote', 'add', 'origin', repo); }
    return instance;
  };
  return {base, repo, cache, commit, local, remote};
}
async function mock(t, replies) {
  const requests = [];
  const server = createServer(async (req, res) => {
    const chunks = []; for await (const chunk of req) chunks.push(chunk);
    const body = Buffer.concat(chunks).toString();
    requests.push({method: req.method, url: req.url, authorization: req.headers.authorization, body: body ? JSON.parse(body) : null});
    const [code, value] = replies.shift() ?? [500, {}];
    res.writeHead(code, {'content-type': 'application/json', ...(code === 429 ? {'retry-after': '60'} : {})});
    res.end(JSON.stringify(value));
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  t.after(() => new Promise((resolve, reject) => server.close(error => error ? reject(error) : resolve())));
  return {url: `http://127.0.0.1:${server.address().port}/`, requests, replies};
}
const prReplies = base => [
  [200, {object: {sha: base}}], [200, {tree: {sha: 'base-tree'}}], [201, {sha: 'blob'}],
  [201, {sha: 'tree'}], [201, {sha: 'a'.repeat(40)}], [200, {object: {sha: base}}],
  [201, {ref: 'proposal'}], [201, {html_url: 'https://github.com/fixture/private/pull/1'}],
];

test('reads are explicit, scoped, pinned and binary safe', async t => {
  const f = fixture(t), repo = await f.local();
  await assert.rejects(repo.read('settings.json'), CacheMissError);
  const first = (await repo.sync()).activeSha;
  const file = await repo.read('settings.json');
  assert.deepEqual(file.json(), {enabled: true}); assert.equal(file.commitSha, first);
  assert.equal(file.provenance.mountPath, 'data');
  assert.deepEqual((await repo.read('binary.bin')).data, Buffer.from([0, 255, 128]));
});

test('text decoding, list/search, validation and path errors', async t => {
  const f = fixture(t), repo = await f.local(); await repo.sync();
  const binary = await repo.read('binary.bin'); assert.throws(() => binary.text, TypeError);
  assert.equal((await repo.list()).length, 3);
  assert.deepEqual(await repo.search('30 days'), ['policy.md']);
  assert.equal(await repo.validate(), true);
  await assert.rejects(repo.read('../outside.txt'), InvalidPathError);
  await assert.rejects(repo.read('policy.md', 'HEAD'), InvalidPathError);
});

test('unchanged reads, retention and rejected updates survive reopening', async t => {
  const f = fixture(t), repo = await f.local(); const first = (await repo.sync()).activeSha;
  writeFileSync(join(f.repo, 'data/settings.json'), '{"enabled":false}'); const second = f.commit();
  assert.equal((await repo.read('settings.json')).commitSha, first);
  await repo.sync(); assert.deepEqual((await repo.read('settings.json', first)).json(), {enabled: true});
  await repo.pin(first); assert.ok((await repo.status()).pinned.includes(first)); await repo.unpin(first);
  assert.equal((await repo.history()).length, 2); assert.equal((await repo.prune()).activeSha, second);
  writeFileSync(join(f.repo, 'data/settings.json'), 'bad JSON'); f.commit();
  await assert.rejects(repo.sync(), ValidationError);
  assert.equal((await (await f.local()).read('settings.json')).commitSha, second);
  assert.equal((await repo.status()).stale, true);
  await repo.clearCache(); await assert.rejects(repo.read('settings.json'), CacheMissError);
});

test('warm offline reads never invoke the token provider', async t => {
  const f = fixture(t); let calls = 0;
  const repo = await f.remote({tokenProvider: async () => {calls++; return 'test-secret';}});
  const first = (await repo.sync()).activeSha;
  await repo.read('policy.md'); await repo.list(); await repo.status(); assert.equal(calls, 1);
  git(join(f.cache, readdirSync(f.cache)[0], 'objects.git'), 'remote', 'set-url', 'origin', join(f.base, 'missing'));
  await assert.rejects(repo.sync());
  assert.equal((await repo.read('policy.md')).commitSha, first);
  assert.equal((await repo.status()).stale, true); assert.equal(calls, 2);
});

test('write permissions and callback failures are safe', async t => {
  const f = fixture(t), repo = await f.remote(); const base = (await repo.sync()).activeSha;
  const options = {expectedBase: base, title: 'Update'};
  await assert.rejects(repo.propose([{operation: 'delete', path: 'policy.md'}], options), PermissionDeniedError);
  await assert.rejects(repo.commit([{operation: 'delete', path: 'policy.md'}], options), PermissionDeniedError);
  const broken = await f.remote({tokenProvider: () => {throw new Error('secret-token-must-not-leak');}});
  await assert.rejects(broken.sync(), error => error instanceof PermissionDeniedError && !error.message.includes('secret-token'));
  assert.equal((await broken.status()).stale, true);
  assert.match((await broken.status()).lastError, /token provider failed/);
});

test('authenticated multi-file PR and atomic direct commit', async t => {
  const f = fixture(t), base = git(f.repo, 'rev-parse', 'HEAD');
  const api = await mock(t, prReplies(base)); let calls = 0;
  const repo = await f.remote({allowProposals: true, allowDirect: true,
    tokenProvider: () => {calls++; return 'test-secret';}, githubApiUrl: api.url});
  await repo.sync();
  const result = await repo.propose([{operation: 'put', path: 'policy.md', content: 'new'},
    {operation: 'delete', path: 'settings.json'}], {expectedBase: base, title: 'Update policy'});
  assert.equal(result.pullRequestUrl, 'https://github.com/fixture/private/pull/1');
  assert.equal((await repo.status()).activeSha, base);
  assert.equal(api.requests.length, 8); assert.equal(calls, 2);
  assert.ok(api.requests.every(req => req.authorization === 'Bearer test-secret'));
  assert.ok(api.requests[3].body.tree.every(entry => entry.path.startsWith('data/')));
  api.replies.push([200, {object: {sha: base}}], [200, {data: {createCommitOnBranch: {commit: {oid: 'b'.repeat(40)}}}}]);
  const direct = await repo.commit([{operation: 'put', path: 'policy.md', content: Buffer.from([0, 255])}],
    {expectedBase: base, title: 'Direct'});
  assert.equal(direct.pullRequestUrl, null);
  assert.equal(api.requests.at(-1).body.variables.input.expectedHeadOid, base);
});

test('stale base, rate limits and provider permissions have typed errors', async t => {
  const f = fixture(t), api = await mock(t, [[200, {object: {sha: 'b'.repeat(40)}}],
    [429, {message: 'test-secret'}], [403, {message: 'test-secret'}]]);
  const repo = await f.remote({allowProposals: true, tokenProvider: () => 'test-secret', githubApiUrl: api.url});
  const base = (await repo.sync()).activeSha;
  for (const Type of [ConflictError, RateLimitError, PermissionDeniedError]) {
    await assert.rejects(repo.propose([{operation: 'put', path: 'policy.md', content: 'new'}],
      {expectedBase: base, title: 'Change'}), error => error instanceof Type && !error.message.includes('test-secret'));
  }
  assert.equal(api.requests.length, 3);
});

test('native work leaves the event loop responsive', async t => {
  const f = fixture(t), repo = await f.local({validation: {commands: [{name: 'slow',
    command: [process.execPath, '-e', 'setTimeout(() => {}, 200)']}]}});
  let ticks = 0; const timer = setInterval(() => ticks++, 10);
  try {
    await repo.sync(); assert.ok(ticks > 5);
    await Promise.all([repo.sync(), repo.sync()]); assert.equal((await repo.history()).length, 1);
  } finally { clearInterval(timer); }
});

test('schema validation and symlinks use the Rust security boundary', async t => {
  const f = fixture(t), repo = await f.local({validation: {schemas: [{path: 'settings.json',
    schema: {type: 'object', required: ['missing']}}]}});
  await assert.rejects(repo.sync(), ValidationError);
  if (process.platform !== 'win32') {
    symlinkSync('../outside.txt', join(f.repo, 'data/link')); f.commit();
    await assert.rejects((await f.local()).sync(), ValidationError);
  }
});

test('host subprocesses remain usable between native Git operations', async t => {
  const f = fixture(t);
  // Node/libuv reconfigures child handling when it starts its own subprocesses.
  // A native library must not depend on owning that process-global signal handler.
  for (let i = 0; i < 3; i++) {
    const repo = await f.local();
    await repo.sync();
    assert.equal(git(f.repo, 'rev-parse', 'HEAD'), (await repo.status()).activeSha);
    const reopened = await f.local();
    assert.equal((await reopened.read('settings.json')).json().enabled, true);
  }
});

test('candidate inspection, bulk snapshots and atomic pins use the native core', async t => {
  const f = fixture(t);
  mkdirSync(join(f.repo, 'data/build'));
  writeFileSync(join(f.repo, 'data/build/invalid.json'), 'invalid'); f.commit();
  const repo = await f.local({excludedPaths: ['build'], limits: {retainedRevisions: 1}});
  const first = (await repo.sync(undefined, {pin: true})).activeSha;
  const data = await repo.snapshot();
  assert.equal(data.commitSha, first);
  assert.equal(Buffer.from(data.files['policy.md']).toString(), 'Refund within 30 days.\n');
  assert.equal(data.files['build/invalid.json'], undefined);
  assert.deepEqual((await repo.status()).pinned, [first]);
  writeFileSync(join(f.repo, 'data/settings.json'), 'invalid'); const broken = f.commit();
  const candidate = await repo.inspectCandidate();
  assert.equal(candidate.commitSha, broken); assert.equal(candidate.rootExists, true); assert.ok(candidate.issues.length);
  assert.equal((await repo.snapshot()).commitSha, first);
});

test('proposal upsert uses expected-head CAS and returns typed PR identity', async t => {
  const f = fixture(t), base = git(f.repo, 'rev-parse', 'HEAD');
  const commit = 'a'.repeat(40);
  const pr = {number: 7, html_url: 'https://github.com/fixture/private/pull/7', state: 'open',
    head: {sha: commit, ref: 'app/sync'}, base: {sha: base, ref: 'main'}};
  const api = await mock(t, [[200, {object: {sha: base}}], [404, {}], [200, []],
    [200, {tree: {sha: 'base-tree'}}], [201, {sha: 'blob'}], [201, {sha: 'tree'}], [201, {sha: commit}],
    [200, {object: {sha: base}}], [200, {node_id: 'repo-node'}], [200, {data: {updateRefs: {clientMutationId: null}}}],
    [201, pr], [200, pr]]);
  const repo = await f.remote({allowProposals: true, githubApiUrl: api.url, tokenProvider: async () => 'test-secret'});
  await repo.sync();
  const result = await repo.upsertProposal([{operation: 'put', path: 'policy.md', content: 'New policy'}],
    {expectedBase: base, title: 'Update', branch: 'app/sync', operationId: 'operation-1'});
  assert.equal(result.outcome, 'created'); assert.equal(result.pullRequestNumber, 7);
  assert.equal(result.proposal.commitSha, commit);
  const cas = api.requests.find(r => r.url === '/graphql').body.variables.input.refUpdates[0];
  assert.equal(cas.beforeOid, '0'.repeat(40)); assert.equal(cas.afterOid, commit);
  assert.equal((await repo.proposalStatus(7)).headSha, commit);
});

test('a source move before proposal publication is a retryable stale-base conflict', async t => {
  const f = fixture(t), base = git(f.repo, 'rev-parse', 'HEAD');
  const api = await mock(t, [[200, {object: {sha: 'b'.repeat(40)}}], [404, {}], [200, []]]);
  const repo = await f.remote({allowProposals: true, githubApiUrl: api.url, tokenProvider: () => 'test-secret'});
  await repo.sync();
  await assert.rejects(repo.upsertProposal([{operation: 'put', path: 'policy.md', content: 'New policy'}],
    {expectedBase: base, title: 'Update', branch: 'app/sync', operationId: 'rejected'}),
    error => error instanceof ConflictError && error.code === 'stale_base');
  assert.ok(api.requests.every(r => r.method === 'GET'));
});
