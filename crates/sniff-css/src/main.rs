//! `sniff` binary: computed styles for LLMs (capture, check, diff).

mod cli;
mod commands;
mod output;

use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    commands::run().await
}
