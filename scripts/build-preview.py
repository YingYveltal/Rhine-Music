#!/usr/bin/env python3
"""Build the committed Preview source and export a signed app, ZIP and manifest."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import plistlib
import shutil
import subprocess
from datetime import datetime, timezone

ROOT = Path(__file__).resolve().parent.parent

def run(*args, **kwargs):
    return subprocess.run(args, cwd=ROOT, check=True, **kwargs)

def output(*args):
    return run(*args, capture_output=True, text=True).stdout.strip()

def digest(path):
    checksum = hashlib.sha256()
    with path.open('rb') as source:
        for block in iter(lambda: source.read(1024 * 1024), b''):
            checksum.update(block)
    return checksum.hexdigest()

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True, help='New delivery directory (outside tracked source)')
    args = parser.parse_args()
    if output('git', 'status', '--porcelain'):
        raise SystemExit('Commit source changes before building Preview; no dirty source is labeled as a commit.')
    if output('uname', '-m') != 'arm64':
        raise SystemExit('This preview build is for Apple Silicon only.')
    if int(output('sw_vers', '-productVersion').split('.')[0]) < 14:
        raise SystemExit('Preview requires macOS 14 or newer for independent WebKit storage.')
    commit = output('git', 'rev-parse', 'HEAD')
    destination = args.output.expanduser().resolve()
    if destination.exists():
        raise SystemExit('Output directory already exists; choose a new version directory.')
    config = ROOT / 'src-tauri/tauri.preview.conf.json'
    target = Path(os.environ.get('CARGO_TARGET_DIR', ROOT / '.local/target')).resolve()
    env = os.environ.copy()
    for key in ['TAURI_CONFIG', 'MUSIC_NATIVE_DATA_DIR', 'MUSIC_DATA_DIR', 'RHINE_QQ_SESSION', 'QQMUSIC_API_KEY', 'RHINE_PREVIEW_QA']:
        env.pop(key, None)
    for candidate in [Path.home()/'.rustup/toolchains/stable-aarch64-apple-darwin/bin', Path.home()/'.cargo/bin']:
        if (candidate/'cargo').is_file():
            env['PATH'] = str(candidate) + os.pathsep + env['PATH']
            break
    env.update(CARGO_TARGET_DIR=str(target), CARGO_BUILD_JOBS='3', MACOSX_DEPLOYMENT_TARGET='14.0')
    run('npm', 'run', 'tauri', '--', 'build', '--bundles', 'app', '--features', 'preview', '--config', str(config), '--', '--locked', env=env)
    if output('git', 'rev-parse', 'HEAD') != commit or output('git', 'status', '--porcelain'):
        raise SystemExit('Source changed during the build; delivery was not exported.')
    built = target / 'release/bundle/macos/Rhine Music Preview.app'
    destination.mkdir(parents=True)
    app = destination / built.name
    run('ditto', str(built), str(app))
    plist_path = app / 'Contents/Info.plist'
    plist = plistlib.loads(plist_path.read_bytes())
    assert plist['CFBundleIdentifier'] == 'com.rhine.music.preview'
    assert plist['LSMinimumSystemVersion'] == '14.0'
    executable = app / 'Contents/MacOS' / plist['CFBundleExecutable']
    assert 'arm64' in output('lipo', '-archs', str(executable))
    run('codesign', '--verify', '--deep', '--strict', '--verbose=2', str(app))
    signature = run('codesign', '--display', '--verbose=4', str(app), capture_output=True, text=True)
    signature_text = signature.stdout + signature.stderr
    assert 'Signature=adhoc' in signature_text
    assert 'Identifier=com.rhine.music.preview' in signature_text
    (destination/'signature.txt').write_text(signature_text)
    inventory = []
    for path in sorted(app.rglob('*')):
        if path.is_symlink():
            inventory.append({'path':str(path.relative_to(app)), 'symlink':os.readlink(path)})
        elif path.is_file():
            inventory.append({'path':str(path.relative_to(app)), 'bytes':path.stat().st_size, 'sha256':digest(path)})
    inventory_path = destination/'app-inventory.json'
    inventory_path.write_text(json.dumps(inventory, indent=2)+'\n')
    archive = destination/'Rhine Music Preview.zip'
    run('ditto', '-c', '-k', '--sequesterRsrc', '--keepParent', str(app), str(archive))
    shutil.copyfile(ROOT/'docs/PREVIEW.md', destination/'试用说明.md')
    manifest = {'sourceCommit':commit, 'cleanSource':True, 'builtAt':datetime.now(timezone.utc).isoformat(),
        'architecture':'arm64', 'minimumMacOS':'14.0', 'bundleIdentifier':plist['CFBundleIdentifier'],
        'feature':'preview', 'config':'src-tauri/tauri.preview.conf.json', 'configSha256':digest(config),
        'keychainService':'com.rhine.music.preview', 'keychainAccount':'connection',
        'webkitStore':'953EF41C-7A04-42DA-B989-3BB2E67105A8',
        'qaKeychainService':'com.rhine.music.preview.qa', 'qaWebkitStore':'5C46FA23-6A5B-4F36-A28C-0D9A48C30F72',
        'app':app.name, 'zip':archive.name, 'zipSha256':digest(archive),
        'executableSha256':digest(executable), 'infoPlistSha256':digest(plist_path),
        'appInventorySha256':digest(inventory_path), 'signatureVerified':True, 'signature':'ad-hoc; not notarized',
        'runtimeVerification':'pending; see the separate QA report for this exact executable hash'}
    (destination/'manifest.json').write_text(json.dumps(manifest, ensure_ascii=False, indent=2)+'\n')
    print(json.dumps({'destination':str(destination), 'sourceCommit':commit, 'executableSha256':manifest['executableSha256']}, indent=2))

if __name__ == '__main__':
    main()
