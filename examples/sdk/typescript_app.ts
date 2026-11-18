/** Run with npm start from this folder after building sdks/typescript. */
import { Repository, RaadError } from '@amanmibra/raad';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const source = fileURLToPath(new URL('../..', import.meta.url));
// The demo cleans up its cache. Give a production app a persistent cacheDir.
const cache = await mkdtemp(join(tmpdir(), 'raad-node-app-'));
try {
  const remote = process.env.RAAD_EXAMPLE_REPO;
  const repo = await Repository.open({
    ...(remote ? {repository: remote, root: process.env.RAAD_EXAMPLE_ROOT ?? 'sops'}
      : {localPath: source, root: 'examples/sopops/sops'}),
    cacheDir: cache,
    // Replace this with your host's installation-token cache/refresh function.
    tokenProvider: process.env.RAAD_GITHUB_TOKEN ? () => process.env.RAAD_GITHUB_TOKEN! : undefined,
  });
  try { await repo.sync(); }
  catch (error) {
    if (!(error instanceof RaadError) || (await repo.status()).activeSha === null) throw error;
    console.error(`Using the last valid copy: ${error.message}`);
  }
  const status = await repo.status();
  const file = await repo.read('procedures/refunds.md', status.activeSha ?? undefined);
  console.log(JSON.stringify({commit: file.commitSha, stale: status.stale, text: file.text}, null, 2));
} finally { await rm(cache, {recursive: true, force: true}); }
