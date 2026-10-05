#!/usr/bin/env python3
"""Translate the captured, expanded Three shaders; no material reimplementation.

Requires the official glslang and SPIRV-Cross command-line packages.
All input textures and geometry remain local. Output includes SPIR-V reflection
so the Rust renderer can bind the exact captured uniform values and attributes.
"""
import argparse
import json
from pathlib import Path
import re
import subprocess


def run(*args):
    p = subprocess.run(args, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    if p.returncode:
        raise RuntimeError(" ".join(map(str, args)) + "\n" + p.stdout)
    return p.stdout


decl = re.compile(r"(?:layout\s*\([^)]*\)\s*)?\b(in|out)\s+(?:(?:highp|mediump|lowp)\s+)?(\w+)\s+(\w+)(\s*\[\s*(\d+)\s*\])?\s*;")


def translate(capture, output):
    frame = json.loads((capture / "gl-frame.json").read_text())
    output.mkdir(parents=True, exist_ok=True)
    translated = []
    for i, program in enumerate(frame["programs"]):
        prefix = f"program-{i:03}"
        source = {}
        for stage, key in [("vert", "vertex"), ("frag", "fragment")]:
            s = re.sub(r"#version[^\n]*", "#version 450", program[key], count=1)
            if "#version" not in s:
                s = "#version 450\n" + s
                s = re.sub(r"\battribute\b", "in", s)
                s = re.sub(r"\bvarying\b", "out" if stage == "vert" else "in", s)
                s = re.sub(r"\btexture2D\b", "texture", s)
                if stage == "frag":
                    s = s.replace("#version 450", "#version 450\nlayout(location=0) out vec4 rhineFragColor;")
                    s = re.sub(r"\bgl_FragColor\b", "rhineFragColor", s)
            # 'sampler' is a valid ES parameter name but a reserved Vulkan type.
            s = re.sub(r"\bsampler\b", "rhineSampler", s)
            path = output / f"{prefix}.{stage}"
            path.write_text(s)
            source[stage] = run("glslangValidator", "-E", str(path))
        varyings = {}
        loc = 0
        for m in decl.finditer(source["vert"]):
            direction, typ, name, _, n = m.groups()
            if direction == "out":
                varyings[name] = loc
                loc += (int(n) if n else 1) * (int(typ[-1]) if typ.startswith("mat") else 1)
        attribs = {a["name"]: a["location"] for a in program["attributes"]}
        result = {}
        for stage in ["vert", "frag"]:
            def location(m):
                direction, typ, name, arr, _ = m.groups()
                mapping = attribs if stage == "vert" and direction == "in" else varyings
                if stage == "frag" and direction == "out":
                    index = 0
                elif name not in mapping:
                    # Declared but inactive vertex attributes still need unique
                    # locations. glslang removes them from active reflection.
                    index = max([0, *attribs.values()]) + 4
                    mapping[name] = index
                else:
                    index = mapping[name]
                return f"layout(location={index}) {direction} {typ} {name}{arr or ''};"
            s = decl.sub(location, source[stage])
            path = output / f"{prefix}.{stage}"
            path.write_text(s)
            spv = output / f"{prefix}.{stage}.spv"
            run("glslangValidator", "-V", "-R", "--auto-map-bindings", "--auto-map-locations",
                "--set-default-uniform-block", "RhineUniforms", "0", "24", str(path), "-o", str(spv))
            reflection = json.loads(run("spirv-cross", str(spv), "--reflect"))
            msl = output / f"{prefix}.{stage}.metal"
            options = ["--fixup-clipspace", "--flip-vert-y"] if stage == "vert" else []
            run("spirv-cross", str(spv), "--msl", "--msl-version", "20400", "--msl-decoration-binding", *options, "--output", str(msl))
            result[stage] = {"source": msl.name, "reflection": reflection}
        translated.append(result)
        print(f"Translated {i + 1}/{len(frame['programs'])}", flush=True)
    (output / "programs.json").write_text(json.dumps(translated, indent=2))
    (output / "provenance.json").write_text(json.dumps({"capture": frame["id"], "programs": len(translated),
        "glslang": run("glslangValidator", "--version"),
        "method": "Expanded original GLSL -> SPIR-V -> MSL 2.4, explicit matching varyings, OpenGL depth and vertical origin conversion. No material approximation."}, indent=2))


if __name__ == "__main__":
    p = argparse.ArgumentParser()
    p.add_argument("capture", type=Path)
    p.add_argument("output", type=Path)
    a = p.parse_args()
    translate(a.capture, a.output)
