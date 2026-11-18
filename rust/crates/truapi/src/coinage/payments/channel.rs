//! M1 · Channel: cheques posted under a topic and read back, over the
//! statement store.
//!
//! A statement's data is `SCALE(ChannelItem)`, so every implementation posts
//! and reads the same bytes. The channel never interprets the cheque it
//! carries (invariant 1), and keeps it only as long as the transport does
//! (invariant 2). HOP's reference for a cheque too large for a statement is
//! not defined yet (layer 2, section 4), so this channel carries cheques
//! inline only and ignores any other item.

use futures::stream::{BoxStream, StreamExt};
use parity_scale_codec::{Decode, Encode};
use truapi::latest::CoinPaymentCheque;

use super::topic::Topic;

/// What one statement under a topic holds.
#[derive(Clone, Debug, PartialEq, Eq, Encode, Decode)]
pub enum ChannelItem {
	/// The cheque itself.
	Inline(CoinPaymentCheque),
}

/// Why a post did not reach the transport.
#[derive(Clone, Copy, Debug, PartialEq, Eq, derive_more::Display, derive_more::Error)]
pub enum PostError {
	/// The transport did not accept it.
	#[display("the transport did not accept the post")]
	Unreachable,
	/// The cheque does not fit a statement.
	#[display("the cheque does not fit a statement")]
	TooLarge,
}

/// The statement store as the channel uses it: statement data under a topic.
#[async_trait::async_trait]
pub trait Transport: Send + Sync {
	/// Posts `data` under `topic`, signed with the statement-store key of
	/// `product`, retained until `expires_at`, in Unix seconds.
	async fn post(&self, product: &str, topic: Topic, data: Vec<u8>, expires_at: u64) -> Result<(), PostError>;
	/// The data of every statement under `topic` still retained, then of
	/// every new one.
	fn subscribe(&self, topic: Topic) -> BoxStream<'static, Vec<u8>>;
	/// The data of every statement under `topic` still retained.
	async fn fetch(&self, topic: Topic) -> Vec<Vec<u8>>;
}

/// Posts `cheque` under `topic` as `product`, retained until `expires_at`,
/// in Unix seconds.
pub async fn post(
	transport: &dyn Transport,
	product: &str,
	topic: Topic,
	cheque: &CoinPaymentCheque,
	expires_at: u64,
) -> Result<(), PostError> {
	transport.post(product, topic, ChannelItem::Inline(cheque.clone()).encode(), expires_at).await
}

/// Every cheque under `topic` still retained, then every new one. A cheque
/// may arrive more than once; receivers key what they do by its hash.
pub fn subscribe(transport: &dyn Transport, topic: Topic) -> BoxStream<'static, CoinPaymentCheque> {
	transport.subscribe(topic).filter_map(|data| core::future::ready(cheque_of(&data))).boxed()
}

/// Every cheque under `topic` still retained.
pub async fn fetch(transport: &dyn Transport, topic: Topic) -> Vec<CoinPaymentCheque> {
	transport.fetch(topic).await.iter().filter_map(|data| cheque_of(data)).collect()
}

/// The cheque a statement's data holds, if it holds one.
fn cheque_of(data: &[u8]) -> Option<CoinPaymentCheque> {
	match ChannelItem::decode(&mut &data[..]).ok()? {
		ChannelItem::Inline(cheque) => Some(cheque),
	}
}

#[cfg(test)]
mod tests;
