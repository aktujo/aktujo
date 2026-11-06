// SPDX-License-Identifier: MIT
//! Synchronising in the background (`docs/rfc/0004-archive-0.1.md`, decision 12): every space that is synchronised,
//! at a regular interval and whenever an interface asks – after a change, for example. What arrives is read by the
//! [`Worker`], if there is one. How it went is kept per space ([`crate::SpaceInfo::synced`]).
//!
//! A synchronisation holds the workspace's lock while it talks to the relay, so commands wait for it meanwhile.
//! OPEN: splitting it, so that the network is used outside the lock.

use std::{
    sync::{Arc, Mutex, mpsc},
    thread,
    time::Duration,
};

use crate::{Worker, Workspace};

/// Synchronising on a thread of its own: at once, then every `every` and whenever it is woken. It stops when dropped,
/// after the synchronisation it is in.
#[derive(Debug)]
pub struct Syncer {
    wake: Option<mpsc::Sender<()>>,
    thread: Option<thread::JoinHandle<()>>,
}

impl Syncer {
    /// Starts synchronising `workspace`; `worker` is woken when something arrived.
    pub fn start(workspace: Arc<Mutex<Workspace>>, worker: Option<Arc<Worker>>, every: Duration) -> Self {
        let (wake, woken) = mpsc::channel::<()>();
        let thread = thread::Builder::new()
            .name("aktujo-sync".into())
            .spawn(move || {
                loop {
                    let reports = {
                        let mut workspace = workspace.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                        workspace.sync_all()
                    };
                    let received = reports
                        .map(|reports| reports.values().any(|r| r.as_ref().is_ok_and(|r| r.received > 0)))
                        .unwrap_or(false);
                    if received && let Some(worker) = &worker {
                        worker.wake();
                    }
                    match woken.recv_timeout(every) {
                        Ok(()) | Err(mpsc::RecvTimeoutError::Timeout) => {}
                        Err(mpsc::RecvTimeoutError::Disconnected) => return,
                    }
                    while woken.try_recv().is_ok() {}
                }
            })
            .expect("a thread starts");
        Syncer { wake: Some(wake), thread: Some(thread) }
    }

    /// Synchronises now.
    pub fn wake(&self) {
        if let Some(wake) = &self.wake {
            let _ = wake.send(());
        }
    }
}

impl Drop for Syncer {
    fn drop(&mut self) {
        self.wake = None;
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
