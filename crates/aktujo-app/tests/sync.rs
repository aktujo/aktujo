// SPDX-License-Identifier: AGPL-3.0-or-later
//! The archive on several devices, end to end: a person's laptop and phone and someone invited to read, through a
//! relay – documents arrive, rights hold, a removed member gets nothing after, and the space comes back with its
//! recovery code.

use std::net::SocketAddr;

use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use aktujo_app::{AppError, NewFile, Rights, Syncer, Workspace};
use aktujo_crypt::Domain;
use aktujo_relay::{Relay, serve_listener};

/// A relay on a free port of this machine, in a folder of its own; stops with the test.
fn relay(name: &str) -> String {
    let folder = std::env::temp_dir().join(format!("aktujo-app-relay-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    let relay = Relay::open(Domain::AKTUJO, &folder).unwrap();
    let (ready, address) = std::sync::mpsc::channel::<SocketAddr>();
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async move {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            ready.send(listener.local_addr().unwrap()).unwrap();
            serve_listener(relay, listener).await.unwrap();
        });
    });
    format!("http://{}", address.recv().unwrap())
}

fn pdf<'a>(name: &'a str, bytes: &'a [u8]) -> NewFile<'a> {
    NewFile { name, media_type: "application/pdf", bytes }
}

fn titles(workspace: &Workspace, space: &aktujo_app::SpaceId) -> Vec<String> {
    let mut titles: Vec<String> = workspace.documents(space).unwrap().iter().map(|d| d.title.clone()).collect();
    titles.sort();
    titles
}

#[test]
fn devices_and_people_share_a_space() {
    let url = relay("share");
    let mut laptop = Workspace::in_memory("laptop").unwrap();
    let mut phone = Workspace::in_memory("phone").unwrap();
    let mut adviser = Workspace::in_memory("adviser").unwrap();

    // The laptop archives offline, then starts to synchronise.
    let space = laptop.found_space("Home").unwrap();
    let lease = laptop.add_document(&space, pdf("Lease.pdf", b"%PDF-1.7 lease")).unwrap().document;
    let code = laptop.start_sync(&space, &url, Some("Laptop")).unwrap();
    assert_eq!(laptop.spaces().unwrap()[0].rights, Some(Rights::Manage));

    // The person's phone joins and may manage; an adviser joins and may only read.
    let for_phone = laptop.invite(&space, Rights::Manage).unwrap();
    let for_adviser = laptop.invite(&space, Rights::Read).unwrap();
    assert_eq!(phone.join(&for_phone, Some("Phone")).unwrap(), space);
    assert_eq!(adviser.join(&for_adviser, Some("Tax adviser")).unwrap(), space);
    assert!(phone.spaces().unwrap()[0].waiting);
    assert!(matches!(phone.add_document(&space, pdf("x.pdf", b"x")), Err(AppError::NotAllowed(_))));
    assert!(phone.sync(&space).unwrap().waiting);
    let mut added = laptop.sync(&space).unwrap().added;
    added.sort();
    assert_eq!(added, ["Phone", "Tax adviser"]);

    // Everything arrives: the name of the space, the document, its original.
    phone.sync(&space).unwrap();
    adviser.sync(&space).unwrap();
    for other in [&phone, &adviser] {
        let info = &other.spaces().unwrap()[0];
        assert_eq!(info.name.as_deref(), Some("Home"));
        assert!(!info.waiting);
        let document = other.document(&space, &lease).unwrap();
        assert_eq!(other.original(&document.current().original.hash).unwrap(), b"%PDF-1.7 lease");
    }

    // The phone writes; the adviser may not.
    phone.add_document(&space, pdf("Receipt.pdf", b"%PDF-1.7 receipt")).unwrap();
    phone.add_label(&space, &lease, "Flat").unwrap();
    assert!(matches!(adviser.add_label(&space, &lease, "Mine"), Err(AppError::NotAllowed(_))));
    phone.sync(&space).unwrap();
    laptop.sync(&space).unwrap();
    adviser.sync(&space).unwrap();
    assert_eq!(titles(&laptop, &space), ["Lease", "Receipt"]);
    assert_eq!(titles(&adviser, &space), ["Lease", "Receipt"]);
    assert!(laptop.document(&space, &lease).unwrap().labels.contains("Flat"));
    assert_eq!(laptop.members(&space).unwrap().len(), 4, "three devices and the recovery code");

    // The adviser is removed and gets nothing written after.
    assert_eq!(laptop.remove_member(&space, "Tax adviser").unwrap(), "Tax adviser");
    laptop.add_document(&space, pdf("Notice.pdf", b"%PDF-1.7 notice")).unwrap();
    laptop.sync(&space).unwrap();
    assert!(adviser.sync(&space).is_err());
    assert_eq!(titles(&adviser, &space), ["Lease", "Receipt"]);

    // Every device lost: the recovery code brings the space back.
    drop((laptop, phone));
    let mut new_laptop = Workspace::in_memory("new-laptop").unwrap();
    new_laptop.recover(&url, &space, &code, Some("New laptop")).unwrap();
    assert_eq!(titles(&new_laptop, &space), ["Lease", "Notice", "Receipt"]);
    assert_eq!(new_laptop.spaces().unwrap()[0].name.as_deref(), Some("Home"));
}

#[test]
fn devices_have_names_and_synchronise_in_the_background() {
    let url = relay("background");
    let mut laptop = Workspace::in_memory("laptop").unwrap();
    laptop.set_device_name(" Laptop ").unwrap();
    assert_eq!(laptop.device_name().unwrap().as_deref(), Some("Laptop"));
    assert!(laptop.set_device_name("  ").is_err());
    let space = laptop.found_space("Home").unwrap();
    assert_eq!(laptop.spaces().unwrap()[0].synced, None);
    laptop.start_sync(&space, &url, None).unwrap();
    let invitation = laptop.invite(&space, Rights::Write).unwrap();

    let phone = Arc::new(Mutex::new(Workspace::in_memory("phone").unwrap()));
    phone.lock().unwrap().set_device_name("Phone").unwrap();
    phone.lock().unwrap().join(&invitation, None).unwrap();
    laptop.sync(&space).unwrap();
    let members = laptop.members(&space).unwrap();
    assert!(members.iter().any(|m| m.name.as_deref() == Some("Laptop")));
    assert!(members.iter().any(|m| m.name.as_deref() == Some("Phone")), "the phone's own name");
    laptop.add_document(&space, pdf("Lease.pdf", b"%PDF-1.7 lease")).unwrap();
    laptop.sync(&space).unwrap();
    let synced = laptop.spaces().unwrap()[0].synced.clone().unwrap();
    assert!(synced.problem.is_none() && synced.at > 0);

    // The syncer brings the document to the phone on its own.
    let syncer = Syncer::start(phone.clone(), None, Duration::from_secs(3600));
    let deadline = Instant::now() + Duration::from_secs(20);
    while titles(&phone.lock().unwrap(), &space).is_empty() {
        assert!(Instant::now() < deadline, "nothing arrived");
        std::thread::sleep(Duration::from_millis(50));
    }
    drop(syncer);

    // A relay that is gone is a problem of that space, kept – not an end.
    let lost = laptop.found_space("Elsewhere").unwrap();
    assert!(laptop.start_sync(&lost, "http://127.0.0.1:9", None).is_err());
    let reports = laptop.sync_all().unwrap();
    assert!(reports[&space].is_ok());
}
