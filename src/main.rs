mod speedtest;
mod utils;

mod app;
mod source;
mod tui;

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    app::run().await
}
