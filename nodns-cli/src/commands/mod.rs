pub mod add;
pub mod delete;
pub mod key;
pub mod list;
pub mod resolve;

use clap::Subcommand;

use crate::config::Config;

#[derive(Subcommand)]
pub enum Command {
    /// Add a DNS record
    Add(add::CmdArgs),

    /// Delete a DNS record
    Delete(delete::CmdArgs),

    /// List your DNS records from relays
    List(list::CmdArgs),

    /// Resolve a DNS name via DNS lookup
    Resolve(resolve::CmdArgs),

    /// Key management
    Key {
        #[command(subcommand)]
        command: key::KeyCommand,
    },
}

pub async fn run(cmd: Command, cfg: &Config) -> Result<(), String> {
    match cmd {
        Command::Add(args) => add::run(args, cfg).await,
        Command::Delete(args) => delete::run(args, cfg).await,
        Command::List(args) => list::run(args, cfg).await,
        Command::Resolve(args) => resolve::run(args, cfg).await,
        Command::Key { command } => key::run(command),
    }
}
