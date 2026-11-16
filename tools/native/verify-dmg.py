"""Mount the distribution image and verify what users will actually copy."""
import json
import os
import plistlib
import subprocess
import sys
import tempfile
from pathlib import Path
from ds_store import DSStore

root = Path(__file__).resolve().parents[2]
image = Path(sys.argv[1]).resolve()
mount = Path(tempfile.mkdtemp(prefix='dmg-check-', dir=root / '.local'))
attached = False
try:
    subprocess.run(['hdiutil', 'attach', '-readonly', '-nobrowse', '-mountpoint', str(mount), str(image)], check=True, stdout=subprocess.DEVNULL)
    attached = True
    app = mount / 'ViGo.app'
    info = plistlib.loads((app / 'Contents/Info.plist').read_bytes())
    assert info['CFBundleIdentifier'] == 'com.vigo.desktop'
    assert info['CFBundlePackageType'] == 'APPL'
    assert (app / 'Contents/PkgInfo').read_bytes() == b'APPL????'
    assert os.stat(app / 'Contents/MacOS/ViGo').st_mode & 0o111
    assert os.readlink(mount / '应用程序') == '/Applications'
    icon = app / 'Contents/Resources' / info['CFBundleIconFile']
    subprocess.run([sys.executable, str(root / 'tools/native/verify-icon.py'), str(icon)], check=True)
    subprocess.run(['codesign', '--verify', '--deep', '--strict', str(app)], check=True)
    with DSStore.open(str(mount / '.DS_Store'), 'r') as store:
        assert store['ViGo.app']['Iloc'] == (166, 192)
        assert store['应用程序']['Iloc'] == (474, 192)
        assert store['.']['icvp']['backgroundType'] == 2
        assert store['.']['icvp']['iconSize'] == 96
    print(json.dumps({'version': info['CFBundleShortVersionString'], 'mountedSignature': True, 'iconSizes': True, 'applicationsLink': True, 'installationLayout': True}))
finally:
    if attached:
        subprocess.run(['hdiutil', 'detach', str(mount)], check=True, stdout=subprocess.DEVNULL)
    mount.rmdir()
