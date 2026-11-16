//! What the coinage layer needs from the installed wallet: the recovery
//! phrase's entropy, which derives every coinage key, the person whose free
//! tokens pay for unloads, and the statement store its payments travel over.

use std::sync::Arc;

use super::{WalletAccountHolder, allowance::AllowanceAllocationError};
use crate::{
	host_internal::sso_messages::OnExistingAllowancePolicy,
	host_logic::statement_store::{
		StatementBatches, StatementField, StatementStoreApi, StatementWatchError, TopicFilter,
		sign_statement_fields,
	},
	platform::ProductId,
	runtime::{authority::AccountHolder, statement_store_rpc::StatementSubmitError},
};
use crate::runtime::{
	authority::{AuthorityError, StatementStoreAllowanceKey},
	statement_allowance::{
		CollectionCandidate, FixedPersonhoodSigner, PersonhoodSigner, collection::PersonhoodCollection,
		find_including_rings,
		ring::{RingParams, read_ring_revision},
	},
};

impl WalletAccountHolder {
	/// The installed wallet's recovery-phrase entropy.
	pub fn coinage_entropy(&self) -> Result<Vec<u8>, AuthorityError> {
		self.with_installed_keys(|keys| Ok(keys.entropy.to_vec()))
	}

	/// The person behind the installed wallet: a signer over its personhood
	/// keys, the strongest ring that includes one of them, and that ring's
	/// root revision. None while no wallet is installed, the chain cannot be
	/// read, or no ring includes the person yet.
	pub async fn coinage_person(&self) -> Option<(Arc<dyn PersonhoodSigner>, RingParams, u32)> {
		let candidates: Vec<CollectionCandidate> = self
			.with_installed_keys(|keys| {
				Ok(PersonhoodCollection::ALL
					.iter()
					.map(|collection| CollectionCandidate {
						collection: *collection,
						entropy: *keys.personhood_entropy(*collection),
					})
					.collect())
			})
			.ok()?;
		let signer = FixedPersonhoodSigner::new(candidates).await.ok()?;
		let client = self.services.statement_store.chain_client("coinage personhood").await.ok()?;
		let people = self.services.chain_context.get(&client).await.ok()?;
		let rings =
			find_including_rings(client.rpc(), &people.metadata, &signer, &PersonhoodCollection::ALL, u32::MAX)
				.await
				.ok()?;
		let ring = rings.into_iter().next()?;
		let revision =
			read_ring_revision(client.rpc(), &people.metadata, ring.collection, ring.ring_index, &ring.block_hash)
				.await
				.ok()?;
		Some((Arc::new(signer), ring, revision))
	}

	/// Posts `data` under `topic`, signed with `product`'s statement-store
	/// key, which the wallet registers for the period if it has not yet,
	/// retained until `expires_at`, in Unix seconds.
	pub async fn coinage_post(
		&self,
		product: &ProductId,
		topic: [u8; 32],
		data: Vec<u8>,
		expires_at: u64,
	) -> Result<(), CoinagePostError> {
		let session = self.current_session().ok_or(AuthorityError::Disconnected)?;
		let allocation =
			self.allocate_statement_store_allowance(&session, product, OnExistingAllowancePolicy::Ignore).await?;
		let key = StatementStoreAllowanceKey::from_secret_bytes(allocation.secret)?;
		let fields =
			vec![StatementField::Expiry(expires_at << 32), StatementField::Topic1(topic), StatementField::Data(data)];
		let signed = sign_statement_fields(*key.as_secret_bytes(), key.public_key, fields)
			.map_err(CoinagePostError::Unsigned)?;
		let statement = parity_scale_codec::Encode::encode(&signed);
		Ok(self.services.statement_store.submit(&statement, "coinage cheque").await?)
	}


	/// The statements under `topic`: those retained at once, then each new one.
	pub async fn coinage_watch(&self, topic: [u8; 32]) -> Result<StatementBatches, StatementWatchError> {
		self.services.statement_store.start_statement_watch(TopicFilter::MatchAll(vec![topic])).await
	}
}

/// Why a coinage statement was not posted.
#[derive(Debug, derive_more::Display, derive_more::Error, derive_more::From)]
pub enum CoinagePostError {
	/// No wallet session, or the product's key could not be read.
	Authority(AuthorityError),
	/// The product's statement allowance could not be allocated.
	Allowance(AllowanceAllocationError),
	/// The statement could not be signed.
	#[display("the statement could not be signed: {_0}")]
	#[from(ignore)]
	Unsigned(#[error(not(source))] String),
	/// The statement store refused it.
	Submit(StatementSubmitError),
}
