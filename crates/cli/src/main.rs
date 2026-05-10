//! Argos CLI entry point.
//!
//! Subcommands are stubbed for the bootstrap phase. Real implementations
//! arrive in Epic E7 (`tasks/P1_E7_cli_runner.md`).

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "argos", version, about = "Argos — git-native API client", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Run a collection or single request.
    Run {
        /// Path to collection or request file.
        path: String,
        /// Override active environment.
        #[arg(long)]
        env: Option<String>,
        /// Stop on first failure.
        #[arg(long)]
        bail: bool,
    },
    /// List requests / collections in the workspace.
    List {
        /// Path to workspace root (defaults to cwd).
        #[arg(default_value = ".")]
        path: String,
    },
    /// Validate a workspace, collection, or request file.
    Validate {
        /// Path to validate.
        path: String,
    },
}

fn main() -> anyhow::Result<()> {
    argos_core::init_tracing();
    let cli = Cli::parse();

    match cli.command {
        None => {
            println!("argos {}", argos_core::version());
            println!("Run `argos --help` to see commands.");
        }
        Some(Commands::Run { path, env, bail }) => {
            tracing::info!(?path, ?env, bail, "run command (not yet implemented)");
            anyhow::bail!("`run` is not yet implemented (Epic E7).");
        }
        Some(Commands::List { path }) => {
            tracing::info!(?path, "list command (not yet implemented)");
            anyhow::bail!("`list` is not yet implemented (Epic E2).");
        }
        Some(Commands::Validate { path }) => {
            tracing::info!(?path, "validate command (not yet implemented)");
            anyhow::bail!("`validate` is not yet implemented (Epic E2).");
        }
    }

    Ok(())
}
