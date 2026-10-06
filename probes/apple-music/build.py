#!/usr/bin/env python3
"""Build a standalone ad-hoc signed Apple Events probe with no developer token."""
import hashlib,json,plistlib,subprocess
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
OUT=ROOT/'.local/apple-probe'
APP=OUT/'Rhine Apple Music Probe.app'
def run(*args):
    subprocess.run(args,cwd=ROOT,check=True)
def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()
contents=APP/'Contents';(contents/'MacOS').mkdir(parents=True,exist_ok=True)
evidence=OUT/'evidence';evidence.mkdir(exist_ok=True)
info={'CFBundleIdentifier':'com.rhine.music.appleprobe','CFBundleName':'Rhine Apple Music Probe','CFBundleExecutable':'apple-music-probe','CFBundlePackageType':'APPL','CFBundleVersion':'1','CFBundleShortVersionString':'0.1','LSMinimumSystemVersion':'14.0','NSHighResolutionCapable':True,'NSAppleEventsUsageDescription':'读取“音乐”App 的歌单与曲目，并控制您选择的歌曲播放。不会修改资料库。','RhineProbeEvidenceDirectory':str(evidence)}
(contents/'Info.plist').write_bytes(plistlib.dumps(info))
entitlements=OUT/'entitlements.plist';entitlements.write_bytes(plistlib.dumps({'com.apple.security.automation.apple-events':True}))
exe=contents/'MacOS/apple-music-probe'
run('xcrun','clang','-fobjc-arc','-fmodules','-Wall','-Wextra','-Wno-unused-parameter','-mmacosx-version-min=14.0','-framework','Cocoa','-framework','ScriptingBridge',str(ROOT/'probes/apple-music/main.m'),'-o',str(exe))
run('codesign','--force','--sign','-','--options','runtime','--entitlements',str(entitlements),str(APP))
run('codesign','--verify','--deep','--strict',str(APP))
commit=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip()
manifest={'sourceCommit':commit,'dirtySource':bool(subprocess.check_output(['git','status','--porcelain'],cwd=ROOT,text=True).strip()),'bundleIdentifier':info['CFBundleIdentifier'],'app':str(APP),'executableSha256':sha(exe),'infoPlistSha256':sha(contents/'Info.plist'),'sourceFiles':{str(p.relative_to(ROOT)):sha(p) for p in sorted((ROOT/'probes/apple-music').glob('*')) if p.is_file()},'signature':'ad-hoc, hardened runtime, Apple Events entitlement; not notarized','runtimeVerified':False}
(OUT/'manifest.json').write_text(json.dumps(manifest,ensure_ascii=False,indent=2)+'\n')
print(json.dumps(manifest,ensure_ascii=False,indent=2))
