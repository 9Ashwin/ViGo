const { app, BrowserWindow, nativeImage, systemPreferences } = require('electron');
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
  const icon = nativeImage.createFromBuffer(readFileSync(resolve('src/renderer/assets/app-icon.png')));
  writeFileSync(resolve('build/icon.png'), icon.resize({ width: 1024, height: 1024, quality: 'best' }).toPNG());
  const template = `<svg xmlns="http://www.w3.org/2000/svg" width="128" height="128" viewBox="0 0 128 128"><path d="M17 22H42L64 88 86 22H111L77 110H51Z" fill="black" fill-rule="evenodd"/></svg>`;
  const tray = await render(template, 128);
  for (const size of [16,32]) writeFileSync(resolve(`build/trayTemplate${size===32?'@2x':''}.png`), tray.resize({width:size,height:size,quality:'best'}).toPNG());
  console.log('Rendered ViGo application and template icons');
  app.quit();
}).catch((error) => { console.error(error); app.exit(1); });
