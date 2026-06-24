//! kittenscrew — Rust core for the kitten plugin.
//!
//! Wraps squeez (binary + hook scripts) w/ own hooks. Adds spec/plan management,
//! kitty:says() visual wrapper, per-project config. See SPEC.md.

use clap::Parser;
use std::process::ExitCode;

mod check;
mod cli;
mod commands;
mod compression;
mod config;
mod docs;
mod drift;
mod driver;
mod error;
mod gate;
mod hook;
mod init;
mod intake;
mod kitty;
mod plan;
mod score;
mod spec;
mod squeez;
mod store;

fn main() -> ExitCode {
    let cli = cli::Cli::parse();
    match commands::run(cli) {
        Ok(()) => ExitCode::from(0),
        Err(e) => {
            eprintln!("kittenscrew: error: {e}");
            ExitCode::from(e.exit_code())
        }
    }
}
