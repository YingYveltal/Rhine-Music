#!/usr/bin/env python3
"""Build the native MusicKit experiment with the existing probe identity."""
import hashlib,json,plistlib,subprocess
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
OUT=ROOT/'.local/musickit-probe'
APP=OUT/'Rhine Apple Music Probe.app'
def run(*args):subprocess.run(args,cwd=ROOT,check=True)
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
contents=APP/'Contents';(contents/'MacOS').mkdir(parents=True,exist_ok=True)
evidence=OUT/'evidence';evidence.mkdir(exist_ok=True)
info={'CFBundleIdentifier':'com.rhine.music.appleprobe','CFBundleName':'Rhine Apple Music Probe','CFBundleExecutable':'musickit-probe','CFBundlePackageType':'APPL','CFBundleVersion':'2','CFBundleShortVersionString':'0.2','LSMinimumSystemVersion':'14.0','NSHighResolutionCapable':True,'NSAppleMusicUsageDescription':'读取您现有音乐资料库中的少量歌曲样本，并验证原生音乐播放及队列。不会修改资料库。','RhineProbeEvidenceDirectory':str(evidence)}
(contents/'Info.plist').write_bytes(plistlib.dumps(info))
exe=contents/'MacOS/musickit-probe'
run('xcrun','swiftc','-parse-as-library','-swift-version','5','-target','arm64-apple-macos14.0','-framework','Cocoa','-framework','MusicKit',str(ROOT/'probes/apple-musickit/main.swift'),'-o',str(exe))
run('codesign','--force','--sign','-','--options','runtime',str(APP))
run('codesign','--verify','--deep','--strict',str(APP))
manifest={'sourceCommit':subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),'dirtySource':bool(subprocess.check_output(['git','status','--porcelain'],cwd=ROOT,text=True).strip()),'bundleIdentifier':info['CFBundleIdentifier'],'app':str(APP),'executableSha256':sha(exe),'infoPlistSha256':sha(contents/'Info.plist'),'sourceFiles':{str(p.relative_to(ROOT)):sha(p) for p in sorted((ROOT/'probes/apple-musickit').glob('*')) if p.is_file()},'sdk':subprocess.check_output(['xcrun','--show-sdk-version'],text=True).strip(),'os':subprocess.check_output(['sw_vers'],text=True).strip(),'signature':'ad-hoc, hardened runtime, no custom entitlements, no team or provisioning profile','runtimeVerified':False}
(OUT/'manifest.json').write_text(json.dumps(manifest,ensure_ascii=False,indent=2)+'\n')
print(json.dumps(manifest,ensure_ascii=False,indent=2))
