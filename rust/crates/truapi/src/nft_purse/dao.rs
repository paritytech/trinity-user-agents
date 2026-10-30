//! Queries over the tables Layer 2 of NFT purses owns.

use rusqlite::{Transaction, named_params};
use truapi::latest::NftPurseReceiveKey;

use crate::{nft::PurseId, platform::ProductId, store::DbError};

const INSERT_RECEIVE_KEY: &str =
	"INSERT INTO nft_receive_key (key, purse, creator) VALUES (:key, :purse, :creator)";

/// Records that `creator` made `key`, whose cheques go into `purse`.
pub fn insert_receive_key(
	tx: &Transaction<'_>,
	key: &NftPurseReceiveKey,
	purse: PurseId,
	creator: &ProductId,
) -> Result<(), DbError> {
	let params = named_params! { ":key": key.0, ":purse": purse.0, ":creator": creator.as_str() };
	tx.prepare_cached(INSERT_RECEIVE_KEY)?.execute(params)?;
	Ok(())
}
