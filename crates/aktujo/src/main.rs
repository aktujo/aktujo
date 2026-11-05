// SPDX-License-Identifier: AGPL-3.0-or-later
//! The command line of aktujo: the terminal interface, the API with the web app, and the API's description.

use clap::{Parser, Subcommand};

use aktujo_app::Workspace;

/// Your documents, kept safe – end to end.
#[derive(Debug, Parser)]
#[command(name = "aktujo", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// The terminal interface.
    Tui,
    /// The API and the web app on this device.
    Serve {
        /// The port on 127.0.0.1.
        #[arg(long, default_value_t = 8071)]
        port: u16,
    },
    /// Prints the API's description in OpenAPI – what api/openapi.json holds.
    Openapi,
}

fn main() -> anyhow::Result<()> {
    match Cli::parse().command {
        Command::Tui => aktujo_tui::run(&mut Workspace::new())?,
        Command::Serve { port } => {
            let ready = |address: std::net::SocketAddr| println!("aktujo at http://{address} – Ctrl-C ends");
            tokio::runtime::Runtime::new()?.block_on(aktujo_api::serve(Workspace::new(), port, ready))?;
        }
        Command::Openapi => println!("{}", aktujo_api::openapi()),
    }
    Ok(())
}
