use mint_render::Renderer;

fn main() -> std::process::ExitCode {
    let renderer = match Renderer::offscreen(1, 1) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("offscreen renderer: {e}");
            return std::process::ExitCode::FAILURE;
        }
    };
    let info = renderer.info();
    println!("{}|{}|{}", info.adapter, info.backend, info.driver);
    std::process::ExitCode::SUCCESS
}
