use anyhow::{bail, Result};
use std::path::Path;
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 4 {
        bail!("compile-frame <capture> <output> <tool-directory>");
    }
    rhine_metal_lab::shader_translate::translate(
        Path::new(&args[1]),
        Path::new(&args[2]),
        Path::new(&args[3]),
    )
}
