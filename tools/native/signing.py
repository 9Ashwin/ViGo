"""Fixed public release identity. Private material is accepted only from CI secrets."""
import base64
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[2]
CERT = ROOT / 'native/release-signing.pem'
APP_ID = 'com.vigo.desktop'


def run(args, env=None):
    result = subprocess.run(args, capture_output=True, text=True, env=env)
    if result.returncode:
        # Never include arguments: security/import and PKCS12 may take secret passwords.
        raise RuntimeError(f'{Path(args[0]).name} failed (exit {result.returncode})')
    return result.stdout + result.stderr


def fingerprint():
    text = run(['/usr/bin/openssl', 'x509', '-in', str(CERT), '-noout', '-fingerprint', '-sha1'])
    value = text.strip().split('=')[-1].replace(':', '').lower()
    if not re.fullmatch('[0-9a-f]{40}', value):
        raise RuntimeError('Invalid pinned release certificate')
    return value


def requirement():
    return f'identifier "{APP_ID}" and certificate leaf = H"{fingerprint()}"'


def setup():
    encoded = os.environ.get('VIGO_RELEASE_P12_BASE64', '')
    password = os.environ.get('VIGO_RELEASE_P12_PASSWORD', '')
    if not encoded or not password:
        raise RuntimeError('Release signing secrets are required; refusing an ad-hoc main build')
    raw = base64.b64decode(encoded, validate=True)
    if len(raw) > 128 * 1024:
        raise RuntimeError('Unexpected signing identity size')
    directory = Path(tempfile.mkdtemp(prefix='vigo-signing-', dir=os.environ['RUNNER_TEMP']))
    os.chmod(directory, 0o700)
    p12 = directory / 'identity.p12'
    keychain = directory / 'release.keychain-db'
    p12.write_bytes(raw)
    os.chmod(p12, 0o600)
    import secrets
    keychain_password = secrets.token_hex(32)
    try:
        run(['/usr/bin/security', 'create-keychain', '-p', keychain_password, str(keychain)])
        run(['/usr/bin/security', 'unlock-keychain', '-p', keychain_password, str(keychain)])
        run(['/usr/bin/security', 'set-keychain-settings', '-lut', '21600', str(keychain)])
        run(['/usr/bin/security', 'import', str(p12), '-k', str(keychain), '-P', password,
             '-T', '/usr/bin/codesign'])
        run(['/usr/bin/security', 'set-key-partition-list', '-S', 'apple-tool:,apple:,codesign:',
             '-s', '-k', keychain_password, str(keychain)])
        identities = run(['/usr/bin/security', 'find-identity', '-p', 'codesigning', str(keychain)])
        if fingerprint() not in identities.lower():
            raise RuntimeError('Secret identity does not match the pinned public certificate')
        with Path(os.environ['GITHUB_ENV']).open('a') as stream:
            stream.write(f'VIGO_SIGN_IDENTITY={fingerprint()}\nVIGO_SIGN_KEYCHAIN={keychain}\nVIGO_RELEASE_SIGNED=1\n')
        print('Fixed ViGo release identity imported into an isolated temporary keychain.')
    except Exception:
        subprocess.run(['/usr/bin/security', 'delete-keychain', str(keychain)], capture_output=True)
        shutil.rmtree(directory)
        raise
    finally:
        p12.unlink(missing_ok=True)


def cleanup():
    value = os.environ.get('VIGO_SIGN_KEYCHAIN')
    if not value:
        return
    keychain = Path(value).resolve()
    parent = Path(os.environ['RUNNER_TEMP']).resolve()
    if not keychain.is_relative_to(parent) or not keychain.parent.name.startswith('vigo-signing-'):
        raise RuntimeError('Refusing to remove an unrelated keychain')
    subprocess.run(['/usr/bin/security', 'delete-keychain', str(keychain)], capture_output=True)
    shutil.rmtree(keychain.parent, ignore_errors=True)


def local_build(arguments):
    """Use the project's identity locally too, so local/Release installs never alternate signers."""
    private = Path.home() / '.config/ViGo/release-signing'
    local_root = ROOT / '.local/signing'
    local_root.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='build-', dir=local_root) as temporary:
        os.environ['RUNNER_TEMP'] = temporary
        os.environ['GITHUB_ENV'] = str(Path(temporary) / 'signing-env')
        os.environ['VIGO_RELEASE_P12_BASE64'] = base64.b64encode((private / 'release-identity.p12').read_bytes()).decode()
        os.environ['VIGO_RELEASE_P12_PASSWORD'] = (private / 'p12-password').read_text()
        try:
            setup()
            for line in Path(os.environ['GITHUB_ENV']).read_text().splitlines():
                name, value = line.split('=', 1)
                os.environ[name] = value
            os.environ.pop('VIGO_RELEASE_P12_BASE64', None)
            os.environ.pop('VIGO_RELEASE_P12_PASSWORD', None)
            result = subprocess.run(['sh', str(ROOT / 'tools/native/bundle.sh'), *arguments])
            if result.returncode:
                raise RuntimeError('Local release build failed')
        finally:
            cleanup()


def stamp(app, identity):
    mode = 'self-signed' if os.environ.get('VIGO_RELEASE_SIGNED') == '1' else 'ad-hoc' if identity == '-' else 'local'
    info = {'mode': mode, 'stable': mode != 'ad-hoc'}
    if mode == 'self-signed':
        info['certificateSHA1'] = fingerprint()
    (app / 'Contents/Resources/signing-info.json').write_text(json.dumps(info, indent=2) + '\n')


def verify(app, release=False):
    run(['/usr/bin/codesign', '--verify', '--deep', '--strict', str(app)])
    mode = json.loads((app / 'Contents/Resources/signing-info.json').read_text())['mode']
    if release and mode != 'self-signed':
        raise RuntimeError('A public release must use the fixed release certificate')
    if mode == 'self-signed':
        expected = 'designated => ' + requirement()
        for path in [app, app / 'Contents/Resources/system-helper']:
            text = run(['/usr/bin/codesign', '-d', '-r-', str(path)])
            dr = next((line for line in text.splitlines() if line.startswith('designated => ')), '')
            if dr != expected:
                raise RuntimeError('Release identity changed or was weakened')
            run(['/usr/bin/codesign', '--verify', '--strict', '-R', '=' + requirement(), str(path)])
        print('App and helper share the pinned, certificate-bound ViGo identity.')
    else:
        print(f'{mode} build verified; public release identity not claimed.')


def main():
    mode = sys.argv[1]
    if mode == 'setup':
        setup()
    elif mode == 'local-build':
        local_build(sys.argv[2:])
    elif mode == 'cleanup':
        cleanup()
    elif mode == 'requirement':
        print('=designated => ' + requirement())
    elif mode == 'stamp':
        stamp(Path(sys.argv[2]), sys.argv[3])
    elif mode == 'verify':
        verify(Path(sys.argv[2]), '--release' in sys.argv[3:])
    else:
        raise RuntimeError('Unknown signing action')


if __name__ == '__main__':
    try:
        main()
    except Exception as error:
        # Only our deliberately credential-free exception messages leave this script.
        print(f'ViGo signing: {error}', file=sys.stderr)
        sys.exit(1)
