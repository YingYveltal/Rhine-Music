//! Translate expanded original Three.js GLSL through the official Khronos tools.
//! All files remain in the local app-data directory.
use anyhow::{bail, Context, Result};
use regex::{Captures, Regex};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn run(exe: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new(exe)
        .args(args)
        .output()
        .with_context(|| format!("Launch {}", exe.display()))?;
    if !output.status.success() {
        bail!(
            "{}: {}{}",
            exe.display(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    String::from_utf8(output.stdout).context("Compiler UTF-8 output")
}
fn tool(dir: &Path, name: &str) -> PathBuf {
    let bundled = dir.join(name);
    if bundled.exists() {
        bundled
    } else {
        Path::new("/opt/homebrew/bin").join(name)
    }
}
pub fn translate(capture: &Path, output: &Path, tools: &Path) -> Result<()> {
    let frame: Value = serde_json::from_slice(&fs::read(capture.join("gl-frame.json"))?)?;
    fs::create_dir_all(output)?;
    let glslang = tool(tools, "glslangValidator");
    let cross = tool(tools, "spirv-cross");
    let declaration = Regex::new(
        r"(?:layout\s*\([^)]*\)\s*)?\b(in|out)\s+(?:(?:highp|mediump|lowp)\s+)?(\w+)\s+(\w+)(\s*\[\s*(\d+)\s*\])?\s*;",
    )?;
    let version = Regex::new(r"#version[^\n]*")?;
    let reserved = Regex::new(r"\bsampler\b")?;
    let mut translated = vec![];
    for (index, program) in frame["programs"]
        .as_array()
        .context("Shader programs")?
        .iter()
        .enumerate()
    {
        let prefix = format!("program-{index:03}");
        let mut sources = HashMap::new();
        for (stage, key) in [("vert", "vertex"), ("frag", "fragment")] {
            let mut source = version
                .replace(
                    program[key].as_str().context("Shader source")?,
                    "#version 450",
                )
                .into_owned();
            if !source.contains("#version") {
                source = format!("#version 450\n{source}");
                for (from, to) in [
                    ("attribute", "in"),
                    ("varying", if stage == "vert" { "out" } else { "in" }),
                    ("texture2D", "texture"),
                ] {
                    source = Regex::new(&format!(r"\b{from}\b"))?
                        .replace_all(&source, to)
                        .into_owned();
                }
                if stage == "frag" {
                    source = source.replace(
                        "#version 450",
                        "#version 450\nlayout(location=0) out vec4 rhineFragColor;",
                    );
                    source = Regex::new(r"\bgl_FragColor\b")?
                        .replace_all(&source, "rhineFragColor")
                        .into_owned();
                }
            }
            source = reserved.replace_all(&source, "rhineSampler").into_owned();
            let file = output.join(format!("{prefix}.{stage}"));
            fs::write(&file, source)?;
            sources.insert(
                stage,
                run(&glslang, &["-E", file.to_str().context("Shader path")?])?,
            );
        }
        let mut varyings = HashMap::<String, u64>::new();
        let mut location = 0;
        for c in declaration.captures_iter(&sources["vert"]) {
            if &c[1] == "out" {
                varyings.insert(c[3].to_string(), location);
                let count = c
                    .get(5)
                    .map(|v| v.as_str().parse::<u64>())
                    .transpose()?
                    .unwrap_or(1);
                let columns = if c[2].starts_with("mat") {
                    c[2][c[2].len() - 1..].parse::<u64>()?
                } else {
                    1
                };
                location += count * columns;
            }
        }
        let mut attributes: HashMap<String, u64> = program["attributes"]
            .as_array()
            .context("Attributes")?
            .iter()
            .map(|a| {
                (
                    a["name"].as_str().unwrap().to_string(),
                    a["location"].as_u64().unwrap(),
                )
            })
            .collect();
        let mut result = serde_json::Map::new();
        for stage in ["vert", "frag"] {
            let source = declaration.replace_all(&sources[stage], |c: &Captures| {
                let direction = &c[1];
                let typ = &c[2];
                let name = &c[3];
                let arr = c.get(4).map(|v| v.as_str()).unwrap_or("");
                let next = attributes.values().copied().max().unwrap_or(0) + 4;
                let mapping = if stage == "vert" && direction == "in" {
                    &mut attributes
                } else {
                    &mut varyings
                };
                let location = if stage == "frag" && direction == "out" {
                    0
                } else {
                    *mapping.entry(name.to_string()).or_insert(next)
                };
                format!("layout(location={location}) {direction} {typ} {name}{arr};")
            });
            let file = output.join(format!("{prefix}.{stage}"));
            let spv = output.join(format!("{prefix}.{stage}.spv"));
            fs::write(&file, source.as_bytes())?;
            run(
                &glslang,
                &[
                    "-V",
                    "-R",
                    "--auto-map-bindings",
                    "--auto-map-locations",
                    "--set-default-uniform-block",
                    "RhineUniforms",
                    "0",
                    "24",
                    file.to_str().unwrap(),
                    "-o",
                    spv.to_str().unwrap(),
                ],
            )?;
            let reflection: Value =
                serde_json::from_str(&run(&cross, &[spv.to_str().unwrap(), "--reflect"])?)?;
            let msl_name = format!("{prefix}.{stage}.metal");
            let msl = output.join(&msl_name);
            let mut args = vec![
                spv.to_str().unwrap(),
                "--msl",
                "--msl-version",
                "20400",
                "--msl-decoration-binding",
            ];
            if stage == "vert" {
                args.extend(["--fixup-clipspace", "--flip-vert-y"]);
            }
            args.extend(["--output", msl.to_str().unwrap()]);
            run(&cross, &args)?;
            result.insert(
                stage.to_string(),
                json!({"source":msl_name,"reflection":reflection}),
            );
        }
        translated.push(Value::Object(result));
    }
    fs::write(
        output.join("programs.json"),
        serde_json::to_vec_pretty(&translated)?,
    )?;
    fs::write(
        output.join("provenance.json"),
        serde_json::to_vec_pretty(
            &json!({"capture":frame["id"],"programs":translated.len(),"glslang":run(&glslang,&["--version"])? ,"method":"Rust coordinator; expanded original GLSL -> SPIR-V -> MSL 2.4. Matching varyings, OpenGL depth and vertical origin conversion; no material approximation."}),
        )?,
    )?;
    Ok(())
}
