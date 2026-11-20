//! [`ChainHeads`] for `ChainRuntime`.

use futures::{
	Stream,
	stream::{self, BoxStream, StreamExt},
};
use sp_crypto_hashing::blake2_256;
use subxt::{config::Header, utils::H256};
use subxt_rpcs::Error as RpcError;

#[cfg(not(target_arch = "wasm32"))]
use crate::chain::Heads;
#[cfg(not(target_arch = "wasm32"))]
use super::best_block;
#[cfg(not(target_arch = "wasm32"))]
use super::finalized_block;
use super::failure;
use crate::{
	chain::{ChainHeads, HashAndNumber, HeadEvent},
	chain_runtime::{ChainRuntime, LegacyConnection, RuntimeFailure},
};

#[cfg(not(target_arch = "wasm32"))]
const HEADS: &str = "chain_heads";
#[cfg(not(target_arch = "wasm32"))]
const FINALIZED_BLOCK: &str = "chain_finalized_block";
const HEAD_EVENTS: &str = "chain_head_events";

type HeadEvents = BoxStream<'static, Result<HeadEvent, RuntimeFailure>>;

#[async_trait::async_trait]
impl ChainHeads for ChainRuntime {
	#[cfg(not(target_arch = "wasm32"))]
	async fn heads(&self, genesis: H256) -> Result<Heads, RuntimeFailure> {
		let legacy = self.legacy(genesis).await?;
		Ok(Heads {
			finalized: finalized_block(&legacy, HEADS).await?,
			best: best_block(&legacy, HEADS).await?,
		})
	}

	#[cfg(not(target_arch = "wasm32"))]
	async fn finalized_block(&self, genesis: H256) -> Result<HashAndNumber, RuntimeFailure> {
		let legacy = self.legacy(genesis).await?;
		finalized_block(&legacy, FINALIZED_BLOCK).await
	}

	async fn head_events(&self, genesis: H256) -> Result<HeadEvents, RuntimeFailure> {
		let legacy = self.legacy(genesis).await?;
		let finalized = finalized_heads(&legacy).await?;
		let best = best_heads(&legacy).await?;
		Ok(stream::select(finalized, best).boxed())
	}
}

/// Finalized heads exactly as the node announces them. Filling gaps by height
/// would ask for hashes a light client cannot give.
async fn finalized_heads(legacy: &LegacyConnection) -> Result<HeadEvents, RuntimeFailure> {
	let headers = legacy
		.methods
		.chain_subscribe_finalized_heads()
		.await
		.map_err(|error| failure(HEAD_EVENTS, error))?;
	Ok(announced(headers, HeadEvent::Finalized))
}

/// Best heads as the node announces them.
async fn best_heads(legacy: &LegacyConnection) -> Result<HeadEvents, RuntimeFailure> {
	let headers = legacy
		.methods
		.chain_subscribe_new_heads()
		.await
		.map_err(|error| failure(HEAD_EVENTS, error))?;
	Ok(announced(headers, HeadEvent::Best))
}

/// Head events for the announced `headers`, each block hashed with
/// Blake2-256 like every block of the chains the core talks to.
fn announced<Headers, H>(headers: Headers, event: fn(HashAndNumber) -> HeadEvent) -> HeadEvents
where
	Headers: Stream<Item = Result<H, RpcError>> + Send + 'static,
	H: Header,
{
	headers
		.map(move |header| {
			let header = header.map_err(|error| failure(HEAD_EVENTS, error))?;
			Ok(event(HashAndNumber {
				hash: H256(blake2_256(&header.encode())),
				number: header.number(),
			}))
		})
		.boxed()
}
