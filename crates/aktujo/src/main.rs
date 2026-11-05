// SPDX-License-Identifier: AGPL-3.0-or-later
//! The command line of aktujo: the terminal interface, the API with the web app, the relay, and the API's
//! description.

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
    /// The relay that devices synchronise their spaces through, end-to-end encrypted: it keeps member lists and
    /// sealed envelopes, never a key. Plain HTTP – put it behind a proxy with TLS when it is reached from outside.
    Relay {
        /// The folder the relay keeps its data in.
        #[arg(long)]
        data: std::path::PathBuf,
        /// The address and port to listen on; 0.0.0.0:8072 for every network of this machine.
        #[arg(long, default_value = "127.0.0.1:8072")]
        listen: std::net::SocketAddr,
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
        Command::Relay { data, listen } => {
            let relay = aktujo_relay::Relay::open(aktujo_crypt::Domain::AKTUJO, &data)?;
            println!("aktujo relay at http://{listen}, data in {} – Ctrl-C ends", data.display());
            tokio::runtime::Runtime::new()?.block_on(aktujo_relay::serve(relay, listen))?;
        }
        Command::Openapi => println!("{}", aktujo_api::openapi()),
    }
    Ok(())
}
