use clap::Parser;

#[derive(Parser, Debug, Clone)]
pub struct Args {
    #[arg(short, long, default_value_t = 10)]
    pub duration: u64,
    #[arg(short, long, default_value_t = 8)]
    pub concurrency: usize,
    #[arg(short, long)]
    pub province: Option<String>,
    #[arg(short, long)]
    pub base_url: Option<String>,
}
