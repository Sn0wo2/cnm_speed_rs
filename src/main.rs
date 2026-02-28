mod speedtest;
mod utils;

mod app;
mod source;
mod tui;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    app::run()
}
