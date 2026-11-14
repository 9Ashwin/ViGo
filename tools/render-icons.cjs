const { app, BrowserWindow, systemPreferences } = require('electron');
const { readFileSync, writeFileSync } = require('node:fs');
const { resolve } = require('node:path');
systemPreferences.registerDefaults({ ApplePersistenceIgnoreState: true });
app.on('window-all-closed', () => {});
async function render(svg, size) {
  const win = new BrowserWindow({ width: size, height: size, show: false, frame: false, transparent: true,
    webPreferences: { offscreen: true, backgroundThrottling: false } });
  await win.loadURL(`data:text/html,${encodeURIComponent(`<html><body style="margin:0;background:transparent">${svg}</body></html>`)}`);
  const image = (await win.webContents.capturePage()).resize({ width: size, height: size, quality: 'best' });
  win.destroy();
  return image;
}
app.whenReady().then(async () => {
  const mark = readFileSync(resolve('src/renderer/assets/vigo.svg'), 'utf8').replace(/<\?xml[^>]*>/, '');
  const appSvg = `<svg xmlns="http://www.w3.org/2000/svg" width="1024" height="1024" viewBox="0 0 1024 1024"><defs><linearGradient id="bg" x2=".8" y2="1"><stop stop-color="#15354e"/><stop offset="1" stop-color="#07121f"/></linearGradient><radialGradient id="light"><stop stop-color="#36cbe4" stop-opacity=".2"/><stop offset="1" stop-color="#36cbe4" stop-opacity="0"/></radialGradient></defs><rect x="68" y="68" width="888" height="888" rx="216" fill="url(#bg)"/><rect x="69" y="69" width="886" height="886" rx="215" fill="none" stroke="#fff" stroke-opacity=".13" stroke-width="2"/><circle cx="512" cy="450" r="385" fill="url(#light)"/><g transform="translate(220 200) scale(4.56)">${mark.replace(/^<svg[^>]*>/,'').replace(/<\/svg>$/,'')}</g></svg>`;
  writeFileSync(resolve('build/icon.png'), (await render(appSvg, 1024)).toPNG());
  const template = `<svg xmlns="http://www.w3.org/2000/svg" width="128" height="128" viewBox="0 0 128 128"><path d="M64 6 116 49 99 81 64 122 29 81 12 49Z M35 39 64 87 93 39H77L64 63 51 39Z" fill="black" fill-rule="evenodd"/></svg>`;
  const tray = await render(template, 128);
  for (const size of [16,32]) writeFileSync(resolve(`build/trayTemplate${size===32?'@2x':''}.png`), tray.resize({width:size,height:size,quality:'best'}).toPNG());
  console.log('Rendered ViGo application and template icons');
  app.quit();
}).catch((error) => { console.error(error); app.exit(1); });
