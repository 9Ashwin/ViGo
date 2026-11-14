import { dirname, join, resolve } from 'node:path';
import { existsSync, readFileSync, readdirSync, statSync } from 'node:fs';
import type { Plugin } from 'vite';
/** Keep license notices beside the code actually bundled into the renderer. */
export function bundledLicenses(): Plugin {
  return {
    name: 'bundled-licenses',
    generateBundle() {
      const roots = new Map<string, { name: string; license?: string; repository?: string | { url?: string } }>();
      const microsoft = readFileSync(resolve('node_modules/@fluentui/react-components/LICENSE'), 'utf8').replace(/^@[^\n]+\n\n/, '');
      for (const id of this.getModuleIds()) {
        if (!id.includes('/node_modules/') || !this.getModuleInfo(id)?.isIncluded) continue;
        let folder = dirname(id.split('?')[0]);
        while (folder.includes('/node_modules')) {
          const file = join(folder, 'package.json');
          if (existsSync(file)) {
            const info = JSON.parse(readFileSync(file, 'utf8')) as { name?: string; license?: string; repository?: string | { url?: string } };
            if (info.name) { roots.set(folder, { ...info, name: info.name }); break; }
          }
          const parent = dirname(folder); if (parent === folder) break; folder = parent;
        }
      }
      const notices: string[] = [];
      for (const [folder, info] of [...roots].sort((a, b) => a[1].name.localeCompare(b[1].name))) {
        const files = readdirSync(folder).filter((name) => /^(licen[cs]e|notice|copyrightnotice)(\..*)?$/i.test(name) && statSync(join(folder, name)).isFile());
        const repository = typeof info.repository === 'string' ? info.repository : info.repository?.url ?? '';
        let text = files.map((name) => readFileSync(join(folder, name), 'utf8')).join('\n\n');
        if (!text && info.license === 'MIT' && repository.includes('microsoft/')) text = microsoft;
        if (!text) this.error(`No bundled license text for ${info.name}`);
        notices.push(`${info.name}\n${'='.repeat(info.name.length)}\n${text}`);
      }
      this.emitFile({ type: 'asset', fileName: 'third-party-licenses.txt', source: notices.join('\n\n\n') });
    },
  };
}
