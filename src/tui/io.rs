use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;

pub fn backend(stdout: io::Stdout) -> CrosstermBackend<io::Stdout> {
    CrosstermBackend::new(stdout)
}

pub fn terminal(
    backend: CrosstermBackend<io::Stdout>,
) -> io::Result<Terminal<CrosstermBackend<io::Stdout>>> {
    Terminal::new(backend)
}
