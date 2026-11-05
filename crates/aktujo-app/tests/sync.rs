// SPDX-License-Identifier: AGPL-3.0-or-later
//! The archive on several devices, end to end: a person's laptop and phone and someone invited to read, through a
//! relay – documents arrive, rights hold, a removed member gets nothing after, and the space comes back with its
//! recovery code.

use std::net::SocketAddr;

use aktujo_app::{AppError, NewFile, Rights, Workspace};
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
