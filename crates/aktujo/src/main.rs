// SPDX-License-Identifier: AGPL-3.0-or-later
//! The command line of aktujo: the terminal interface, the API with the web app, the relay, and the API's
//! description.

use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use anyhow::Context;
use clap::{Args, Parser, Subcommand};

use aktujo_app::{Reader, SystemKeys, Worker, Workspace, default_folder};

/// Your documents, kept safe – end to end.
#[derive(Debug, Parser)]
#[command(name = "aktujo", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

/// Where the documents are.
#[derive(Debug, Args)]
struct Data {
    /// The data folder; by default `aktujo` in the folder for application data of this user. Its key is kept in the
    /// key store of the operating system.
    #[arg(long, conflicts_with = "memory")]
    data: Option<PathBuf>,
    /// A workspace in memory, gone when the program ends – to try it out.
    #[arg(long)]
    memory: bool,
}

impl Data {
    /// The workspace, and reading in the background on it.
    fn open(&self) -> anyhow::Result<(Arc<Mutex<Workspace>>, Arc<Worker>, bool)> {
        let workspace = if self.memory {
            Workspace::in_memory("this-device")?
        } else {
            let folder = match &self.data {
                Some(folder) => folder.clone(),
                None => {
                    default_folder().context("no folder for application data is known here – give one with --data")?
                }
            };
            Workspace::open(&folder, &SystemKeys).with_context(|| format!("the data folder {}", folder.display()))?
        };
        let workspace = Arc::new(Mutex::new(workspace));
        let reader = Reader::builtin()?;
        let ocr = reader.ocr();
        let worker = Arc::new(Worker::start(workspace.clone(), reader));
        Ok((workspace, worker, ocr))
    }
}

#[derive(Debug, Subcommand)]
enum Command {
    /// The terminal interface.
    Tui {
        #[command(flatten)]
        data: Data,
    },
    /// The API and the web app on this device.
    Serve {
        /// The port on 127.0.0.1.
        #[arg(long, default_value_t = 8071)]
        port: u16,
        #[command(flatten)]
        data: Data,
    },
    /// The relay that devices synchronise their spaces through, end-to-end encrypted: it keeps member lists and
    /// sealed envelopes, never a key. Plain HTTP – put it behind a proxy with TLS when it is reached from outside.
    Relay {
        /// The folder the relay keeps its data in.
        #[arg(long)]
        data: PathBuf,
        /// The address and port to listen on; 0.0.0.0:8072 for every network of this machine.
        #[arg(long, default_value = "127.0.0.1:8072")]
        listen: std::net::SocketAddr,
    },
    /// Prints the API's description in OpenAPI – what api/openapi.json holds.
    Openapi,
}

fn main() -> anyhow::Result<()> {
    match Cli::parse().command {
        Command::Tui { data } => {
            let (workspace, worker, _) = data.open()?;
            aktujo_tui::run(workspace, Some(worker))?;
        }
        Command::Serve { port, data } => {
            let (workspace, worker, ocr) = data.open()?;
            let ready = |address: std::net::SocketAddr| println!("aktujo at http://{address} – Ctrl-C ends");
            tokio::runtime::Runtime::new()?.block_on(aktujo_api::serve(workspace, Some(worker), ocr, port, ready))?;
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
