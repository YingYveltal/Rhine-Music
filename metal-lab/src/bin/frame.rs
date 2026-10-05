#[path = "../frame_renderer.rs"]
mod frame_renderer;
fn main() -> anyhow::Result<()> {
    frame_renderer::main()
}
