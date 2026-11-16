//! Fixtures shared by layer 2's tests.

use core::{
	sync::atomic::{AtomicU8, AtomicU64, Ordering},
	time::Duration,
};
use std::sync::Arc;

use futures::stream::{self, BoxStream, StreamExt};
use parking_lot::Mutex;

use super::{
	channel::{PostError, Transport},
	flows::{Draws, PaymentDeps, PaymentParams, Payments},
	topic::Topic,
};
use crate::{coinage::layer::Layer, store::Db};

/// A statement store that keeps every post, and hands out what it holds.
#[derive(Default)]
pub struct Board {
	/// Every post, in order: the product it was signed as, its topic and its
	/// data.
	pub posts: Mutex<Vec<(String, Topic, Vec<u8>)>>,
}

#[async_trait::async_trait]
impl Transport for Board {
	async fn post(&self, product: &str, topic: Topic, data: Vec<u8>, _: u64) -> Result<(), PostError> {
		self.posts.lock().push((product.to_owned(), topic, data));
		Ok(())
	}

	fn subscribe(&self, topic: Topic) -> BoxStream<'static, Vec<u8>> {
		stream::iter(self.under(topic)).boxed()
	}

	async fn fetch(&self, topic: Topic) -> Vec<Vec<u8>> {
		self.under(topic)
	}
}

impl Board {
	fn under(&self, topic: Topic) -> Vec<Vec<u8>> {
		self.posts.lock().iter().filter(|(_, posted, _)| *posted == topic).map(|(_, _, data)| data.clone()).collect()
	}
}

/// Draws that differ on every call.
#[derive(Default)]
pub struct CountingDraws(AtomicU8);

impl Draws for CountingDraws {
	fn draw(&self) -> [u8; 32] {
		[self.0.fetch_add(1, Ordering::SeqCst).wrapping_add(10); 32]
	}
}

/// The parameters tests run with: a one-hour claim timeout, a day to collect
/// refunds, a minute of clock tolerance.
pub const PARAMS: PaymentParams = PaymentParams {
	claim_timeout: Duration::from_secs(3600),
	refund_window: Duration::from_secs(24 * 3600),
	clock_tolerance: Duration::from_secs(60),
	max_claim_timeout: Duration::from_secs(2 * 3600),
	max_claim_validity: Duration::from_secs(600),
};

/// Layer 2 over `layer` and its `store`, posting to `board`, at the network
/// time `clock` holds, in Unix milliseconds.
pub fn payments(layer: Layer, store: Db, board: Arc<Board>, clock: Arc<AtomicU64>) -> Payments {
	Payments::new(PaymentDeps {
		store,
		layer,
		transport: board,
		draws: Arc::new(CountingDraws::default()),
		clock: Arc::new(move || clock.load(Ordering::SeqCst)),
		params: PARAMS,
	})
}
