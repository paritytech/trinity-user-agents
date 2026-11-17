//! The gate new purse ids wait on while backup recovery can still recreate
//! an earlier purse.

use std::sync::{Arc, Mutex};

use futures::{
	FutureExt,
	channel::oneshot,
	future::Shared,
};

/// Holds back new purse ids until backup recovery can no longer recreate an
/// earlier purse at the id a new one would take. An item's purse is also the
/// purse its key is derived under, so a new purse at an old id would take
/// over the old purse's NFTs. Once open, the gate stays open.
#[derive(Clone)]
pub struct PurseIdGate {
	opened: Shared<oneshot::Receiver<()>>,
	opener: Arc<Mutex<Option<oneshot::Sender<()>>>>,
}

impl PurseIdGate {
	/// A gate that opens when [`Self::open`] is first called.
	pub fn closed() -> Self {
		let (opener, opened) = oneshot::channel();
		Self { opened: opened.shared(), opener: Arc::new(Mutex::new(Some(opener))) }
	}

	/// A gate that is open from the start, for a host with nothing to
	/// recover.
	pub fn opened() -> Self {
		let gate = Self::closed();
		gate.open();
		gate
	}

	/// Opens the gate for good. Opening an open gate does nothing.
	pub fn open(&self) {
		let opener = self.opener.lock().expect("the purse id gate's lock is never poisoned").take();
		if let Some(opener) = opener {
			let _ = opener.send(());
		}
	}

	/// Completes once the gate is open.
	pub async fn wait_until_open(&self) {
		let _ = self.opened.clone().await;
	}
}
