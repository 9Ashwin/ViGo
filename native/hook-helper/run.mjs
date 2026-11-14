// `node native/hook-helper/run.mjs build|test`: builds the macOS input helper and runs its C++ tests.
import { spawnSync } from 'node:child_process';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const run = (cmd, args) => {
  const r = spawnSync(cmd, args, { stdio: 'inherit', cwd: here });
  if (r.status !== 0) process.exit(r.status ?? 1);
};

const action = process.argv[2];
if (action !== 'build' && action !== 'test') {
  console.error('usage: node native/hook-helper/run.mjs build|test');
  process.exit(2);
}
if (process.platform !== 'darwin') {
  console.error('ViGo only builds on macOS');
  process.exit(1);
}
run('sh', ['build.sh']);
if (action === 'test') run(join(here, 'build', 'decision_tests'), []);
