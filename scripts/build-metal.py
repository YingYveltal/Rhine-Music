#!/usr/bin/env python3
"""Build an isolated Metal preview with its shader tools; preserve main/QQ apps."""
import json
import os
import platform
from pathlib import Path
import shutil
import subprocess

root = Path(__file__).resolve().parent.parent
variant = os.environ.get("RHINE_PREVIEW_VARIANT", "metal")
if variant not in ("metal", "v2"):
    raise RuntimeError("RHINE_PREVIEW_VARIANT must be metal or v2")
product = "Rhine Music v2" if variant == "v2" else "Rhine Music Metal Preview"
identifier = "com.rhine.music.v2" if variant == "v2" else "com.rhine.music.metal-preview"
release = root / "releases" / variant
work = release / ".build"
# All generated state belongs to this checkout, even when the caller exports a
# shared CARGO_TARGET_DIR/CARGO_HOME/npm cache. Never borrow the old Native tree.
cache = root / ".local" / "rendering-build"
cache.mkdir(parents=True, exist_ok=True)
for name in ("node_modules", "frontend/node_modules"):
    dependency = root / name
    if not dependency.is_dir() or not dependency.resolve().is_relative_to(root):
        raise RuntimeError(f"Install this checkout's dependencies with npm ci first: {name}")
work.mkdir(parents=True, exist_ok=True)
for name in ("frontend", "src-tauri", "qq-connector", "metal-lab"):
    target = work / name
    if target.exists():
        shutil.rmtree(target)
    shutil.copytree(root / name, target,
        ignore=shutil.ignore_patterns("node_modules", "target", "dist", "gen", ".DS_Store"))
for name in ("package.json", "package-lock.json", "LICENSE", "NOTICE.md"):
    shutil.copy2(root / name, work / name)
for name in ("node_modules", "frontend/node_modules"):
    link = work / name
    if link.is_symlink():
        link.unlink()
    if not link.exists():
        link.symlink_to(root / name, target_is_directory=True)

# Official Khronos compiler binaries plus their non-system dynamic libraries.
# Keep their licenses; rewrite install paths only in this generated snapshot.
tools_dir = work / "metal-tools"
if tools_dir.exists():
    shutil.rmtree(tools_dir)
tools_dir.mkdir()
installed = {}
def vendor(source):
    source = Path(source).resolve()
    target = tools_dir / source.name
    if target.name in installed:
        return target
    installed[target.name] = source
    shutil.copy2(source, target)
    target.chmod(target.stat().st_mode | 0o200)
    for line in subprocess.check_output(["otool", "-L", str(source)], text=True).splitlines()[1:]:
        dep = line.strip().split(" (", 1)[0]
        if dep.startswith(("/usr/lib/", "/System/")):
            continue
        if dep.startswith("@rpath/"):
            filename = Path(dep).name
            candidates = [source.parent / filename, source.parent.parent / "lib" / filename]
            resolved = next((p for p in candidates if p.exists()), None)
            if resolved is None:
                raise RuntimeError(f"Unresolved tool dependency: {dep}")
        else:
            resolved = Path(dep)
        vendored = vendor(resolved)
        subprocess.run(["install_name_tool", "-change", dep, "@loader_path/" + vendored.name, str(target)], check=True, capture_output=True)
    if target.suffix == ".dylib":
        subprocess.run(["install_name_tool", "-id", "@loader_path/" + target.name, str(target)], check=True, capture_output=True)
    subprocess.run(["codesign", "--force", "--sign", "-", str(target)], check=True, capture_output=True)
    return target
for binary in ("glslangValidator", "spirv-cross"):
    found = shutil.which(binary)
    if not found:
        raise RuntimeError(f"Install the official {binary} package before building")
    entrypoint = vendor(found)
    if entrypoint.name != binary:
        shutil.copy2(entrypoint, tools_dir / binary)
for package in ("glslang", "spirv-cross", "spirv-tools"):
    prefix = Path(subprocess.check_output(["brew", "--prefix", package], text=True).strip()).resolve()
    license_file = next(p for p in (prefix / "LICENSE", prefix / "LICENSE.txt") if p.exists())
    shutil.copy2(license_file, tools_dir / f"{package}-LICENSE.txt")
(tools_dir / "provenance.json").write_text(json.dumps({name: str(path) for name, path in installed.items()}, indent=2))

manifest = work / "src-tauri/Cargo.toml"
text = manifest.read_text()
assert '[dev-dependencies]' in text and '"protocol-asset"' in text, "QQ manifest changed; review preview injection"
text = text.replace('"protocol-asset"', '"protocol-asset", "macos-private-api"')
text = text.replace('[dev-dependencies]', '''rhine-metal-lab = { path = "../metal-lab" }
metal = "0.31"
objc = "0.2"
core-graphics-types = "0.1"

[dev-dependencies]''')
manifest.write_text(text)
main = work / "src-tauri/src/main.rs"
text = main.read_text()
assert "mod metal_bridge;" not in text, "Metal must remain opt-in in the baseline"
text = "mod metal_bridge;\n" + text
text = text.replace('.invoke_handler(tauri::generate_handler![', '.invoke_handler(tauri::generate_handler![\n            metal_bridge::metal_capabilities, metal_bridge::metal_prepare, metal_bridge::metal_frame, metal_bridge::metal_stop, metal_bridge::metal_profile_slow,')
assert 'metal_bridge::metal_prepare' in text
main.write_text(text)
config_file = work / "src-tauri/tauri.conf.json"
config = json.loads(config_file.read_text())
config.update(productName=product, identifier=identifier)
config["app"]["macOSPrivateApi"] = True
config["app"]["windows"][0].update(title=product, transparent=True, backgroundColor="#00000000")
config["bundle"]["resources"]["../metal-tools/"] = "metal-tools/"
config["bundle"]["shortDescription"] = "Metal 原版着色器实验渲染与 Rust 播放核心；控件仍使用系统 WebView"
config_file.write_text(json.dumps(config, ensure_ascii=False, indent=2))
env = os.environ.copy()
if not shutil.which("cargo"):
    arch = "aarch64" if platform.machine() == "arm64" else "x86_64"
    candidates = [Path.home() / f".rustup/toolchains/stable-{arch}-apple-darwin/bin", Path.home() / ".cargo/bin"]
    compiler = next((p for p in candidates if (p / "cargo").is_file()), None)
    if compiler is None:
        raise RuntimeError("Install a Rust toolchain before building")
    env["PATH"] = str(compiler) + os.pathsep + env["PATH"]
env["CARGO_TARGET_DIR"] = str(release / ".target")
env["CARGO_HOME"] = str(cache / "cargo")
env["npm_config_cache"] = str(cache / "npm")
env["CARGO_BUILD_JOBS"] = "4"
subprocess.run(["npm", "run", "tauri", "--", "build", "--bundles", "app"], cwd=work, env=env, check=True)
app = release / f"{product}.app"
if app.exists():
    shutil.rmtree(app)
shutil.copytree(Path(env["CARGO_TARGET_DIR"]) / f"release/bundle/macos/{product}.app", app, symlinks=True)
subprocess.run(["codesign", "--verify", "--deep", "--strict", str(app)], check=True)
print(f"Metal preview ready: {app}")
