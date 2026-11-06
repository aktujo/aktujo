// SPDX-License-Identifier: AGPL-3.0-or-later
//! The command line of aktujo: the terminal interface, the API with the web app, the relay, and the API's
//! description.

use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

use anyhow::Context;
use clap::{Args, Parser, Subcommand};

use aktujo_app::{Reader, Syncer, SystemKeys, Watcher, Worker, Workspace, default_folder};

/// How often synchronised spaces are synchronised, besides after every change.
const SYNC_EVERY: Duration = Duration::from_secs(60);
/// How often the watched folders are looked at; a file is taken at the second look that finds it unchanged.
const LOOK_EVERY: Duration = Duration::from_secs(5);

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
    /// The workspace, and reading and synchronising in the background on it.
    fn open(&self) -> anyhow::Result<(Arc<Mutex<Workspace>>, aktujo_api::Background)> {
        let workspace = Arc::new(Mutex::new(self.workspace()?));
        let reader = Reader::builtin()?;
        let ocr = reader.ocr();
        let worker = Arc::new(Worker::start(workspace.clone(), reader));
        let syncer = Arc::new(Syncer::start(workspace.clone(), Some(worker.clone()), SYNC_EVERY));
        let watcher = Arc::new(Watcher::start(workspace.clone(), Some(worker.clone()), LOOK_EVERY));
        let background =
            aktujo_api::Background { worker: Some(worker), syncer: Some(syncer), watcher: Some(watcher), ocr };
        Ok((workspace, background))
    }

    /// The workspace alone.
    fn workspace(&self) -> anyhow::Result<Workspace> {
        Ok(if self.memory {
            // A device name of its own, as a data folder gets: two trials can then share a space.
            let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.subsec_nanos();
            Workspace::in_memory(&format!("memory-{:08x}", nanos ^ std::process::id()))?
        } else {
            let folder = match &self.data {
                Some(folder) => folder.clone(),
                None => {
                    default_folder().context("no folder for application data is known here – give one with --data")?
                }
            };
            Workspace::open(&folder, &SystemKeys).with_context(|| format!("the data folder {}", folder.display()))?
        })
    }
}

/// Templates of kinds from a catalog.
#[derive(Debug, Subcommand)]
enum Templates {
    /// Takes a template over into a space: kinds new there are added, kinds a person changed are merged with their
    /// changes kept, kinds gone from the template are kept – what the web app proposes without a choice.
    Take {
        /// The template's ID, e.g. `aktujo.paperwork`.
        id: String,
        /// The space, by its identity.
        #[arg(long)]
        space: String,
        /// The catalog; by default the official one.
        #[arg(long, default_value = aktujo_app::DEFAULT_SOURCE)]
        source: String,
        #[command(flatten)]
        data: Data,
    },
}

/// Exports one space or all, each into a folder or a ZIP of its own in `to`.
fn export(space: Option<&str>, to: &std::path::Path, zip: bool, data: &Data) -> anyhow::Result<()> {
    let workspace = Mutex::new(data.workspace()?);
    let spaces = workspace.lock().map_err(|_| anyhow::anyhow!("the workspace"))?.spaces()?;
    let chosen: Vec<_> = spaces
        .into_iter()
        .filter(|s| space.is_none_or(|wanted| s.id.as_str() == wanted || s.name.as_deref() == Some(wanted)))
        .collect();
    if chosen.is_empty() {
        match space {
            Some(wanted) => anyhow::bail!("no space {wanted:?}"),
            None => anyhow::bail!("there is no space to export"),
        }
    }
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_millis() as u64;
    for info in chosen {
        let name = aktujo_app::export_name(info.name.as_deref(), now);
        let exported = if zip {
            let path = to.join(format!("{name}.zip"));
            let file = std::fs::File::create_new(&path).with_context(|| format!("{}", path.display()))?;
            let mut sink = aktujo_app::Zip::new(std::io::BufWriter::new(file), &name);
            let exported = aktujo_app::export(&workspace, &info.id, &mut sink)?;
            println!("{}", path.display());
            exported
        } else {
            let path = to.join(&name);
            let mut sink = aktujo_app::Folder::create(&path).with_context(|| format!("{}", path.display()))?;
            let exported = aktujo_app::export(&workspace, &info.id, &mut sink)?;
            println!("{}", path.display());
            exported
        };
        print!("  {} documents, {} files", exported.documents, exported.files);
        if exported.missing > 0 {
            print!(", {} originals not on this device", exported.missing);
        }
        println!();
    }
    Ok(())
}

/// Lists the templates of a catalog – read now.
fn list_templates(source: &str) -> anyhow::Result<()> {
    let catalog = aktujo_app::fetch_catalog(source)?;
    println!("{} – {}", catalog.source.name, catalog.source.description);
    for entry in &catalog.templates {
        let kinds: Vec<&str> = entry.kinds.iter().map(|k| k.name.as_str()).collect();
        println!("{:<24} {} – {}", entry.id, entry.name, kinds.join(", "));
    }
    Ok(())
}

/// Takes a template over and says what happened to each kind.
fn take_template(id: &str, space: &str, source: &str, data: &Data) -> anyhow::Result<()> {
    let source = aktujo_app::source_url(source)?;
    let catalog = aktujo_app::fetch_catalog(&source)?;
    let entry =
        catalog.templates.iter().find(|t| t.id == id).with_context(|| format!("no template {id} in {source}"))?;
    let template = aktujo_app::fetch_template(&source, entry)?;
    let mut workspace = data.workspace()?;
    let space = space.parse().map_err(|_| anyhow::anyhow!("{space:?} is not the identity of a space"))?;
    let origin = aktujo_app::TemplateOrigin { source: Some(source), id: Some(id.to_owned()) };
    let plan = workspace.plan(&space, origin, &template)?;
    let changed = workspace.apply(&space, &plan, &Default::default())?;
    for kind in &plan.kinds {
        let conflicts = if kind.conflicts.is_empty() {
            String::new()
        } else {
            let items: Vec<&str> = kind.conflicts.iter().map(|c| c.item.as_str()).collect();
            format!(" – kept here: {}", items.join(", "))
        };
        println!("{:<16} {:?}{conflicts}", kind.id, kind.step);
    }
    println!("{changed} kinds changed");
    Ok(())
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
    /// Exports a space – or every space – in aktujo's own open form: the state of every document as JSON, every
    /// version as it was added, what was read, and the events as they are. Nothing in it is encrypted.
    Export {
        /// The space, by its identity or name; every space if not given.
        #[arg(long)]
        space: Option<String>,
        /// The folder the export goes into, as a folder of its own named after the space and the day.
        #[arg(long, default_value = ".")]
        to: PathBuf,
        /// A ZIP instead of a folder.
        #[arg(long)]
        zip: bool,
        #[command(flatten)]
        data: Data,
    },
    /// Lists the templates of kinds in a catalog, read now – or takes one over into a space.
    Templates {
        /// The catalog; by default the official one.
        #[arg(long, default_value = aktujo_app::DEFAULT_SOURCE)]
        source: String,
        #[command(subcommand)]
        take: Option<Templates>,
    },
}

fn main() -> anyhow::Result<()> {
    match Cli::parse().command {
        Command::Tui { data } => {
            let (workspace, background) = data.open()?;
            aktujo_tui::run(workspace, background.worker, background.syncer, background.watcher)?;
        }
        Command::Serve { port, data } => {
            let (workspace, background) = data.open()?;
            let ready = |address: std::net::SocketAddr| println!("aktujo at http://{address} – Ctrl-C ends");
            tokio::runtime::Runtime::new()?.block_on(aktujo_api::serve(workspace, background, port, ready))?;
        }
        Command::Relay { data, listen } => {
            let relay = aktujo_relay::Relay::open(aktujo_crypt::Domain::AKTUJO, &data)?;
            println!("aktujo relay at http://{listen}, data in {} – Ctrl-C ends", data.display());
            tokio::runtime::Runtime::new()?.block_on(aktujo_relay::serve(relay, listen))?;
        }
        Command::Openapi => println!("{}", aktujo_api::openapi()),
        Command::Templates { source, take: None } => list_templates(&source)?,
        Command::Export { space, to, zip, data } => export(space.as_deref(), &to, zip, &data)?,
        Command::Templates { take: Some(Templates::Take { id, space, source, data }), .. } => {
            take_template(&id, &space, &source, &data)?
        }
    }
    Ok(())
}
