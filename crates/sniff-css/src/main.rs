//! `sniff` binary: computed styles for LLMs (capture, check, diff).

mod cli;
mod commands;
mod input;
mod output;
mod profile;
use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    Box::pin(commands::run()).await
}
