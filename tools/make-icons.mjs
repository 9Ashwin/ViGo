import { createRequire } from 'node:module';
import { spawnSync } from 'node:child_process';
const require = createRequire(import.meta.url);
const result = spawnSync(require('electron'), ['tools/render-icons.cjs'], { stdio: 'inherit' });
process.exit(result.status ?? 1);
