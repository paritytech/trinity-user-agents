//! The Pocket collection in the core database: which cards are in the Pocket,
//! and the newest face drawn for each.
//!
//! The host hands over the cards it pins when the runtime starts and adds the
//! cards the user approves. Products read their own cards and may remove them.
//! The faces their renders draw are kept here, so a card has one to show
//! offline and at the next start.

use core::time::Duration;
use std::sync::Arc;

use futures::{StreamExt, channel::mpsc::UnboundedReceiver, stream::BoxStream};
use parity_scale_codec::Encode;
use rusqlite::{OptionalExtension, Row, Transaction, named_params};
use truapi::latest::{
	GenericError, HostPocketListSubscribeItem, HostPocketRemoveCardError,
	HostPocketRemoveCardRequest, PocketCard, RendererNode,
};

use crate::{
	native::decode_renderer_node,
	platform::{PocketPlatform, ProductContext, ProductId, async_trait},
	store::{Db, DbError},
	time::Timer,
	unix_time::current_unix_millis,
};

/// A card the host pins: listed first, and removable by nobody.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct PinnedPocketCard {
	/// Product that backs the card.
	pub product_id: ProductId,
	/// Card label, unique within the product.
	pub card_id: String,
	/// Title the host shows for the card.
	pub title: String,
}

/// One card in the Pocket, as the host lists it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct PocketEntry {
	/// Product that backs the card.
	pub product_id: String,
	/// Card label, unique within the product.
	pub card_id: String,
	/// Title shown for the card.
	pub title: String,
	/// Pinned by the host, so removable by nobody.
	pub privileged: bool,
}

/// What a removal did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum PocketCardRemoval {
	/// The card was in the Pocket and is gone.
	Removed,
	/// The card was not in the Pocket.
	Absent,
	/// The card is pinned and stays.
	Privileged,
}

/// Least time between two writes of one card's face. A product redraws at
/// frame rate, and the card needs one recent face, not every one.
const FACE_WRITE_INTERVAL: Duration = Duration::from_secs(2);

/// The Pocket collection over the core database.
#[derive(Clone)]
pub struct PocketCards {
	db: Db,
}

impl PocketCards {
	/// The collection in `db`, with `pinned` as the host's pins. A card no
	/// longer pinned stays only if the user added it.
	pub async fn open(db: Db, pinned: Vec<PinnedPocketCard>) -> Result<Self, DbError> {
		db.write(move |tx| {
			tx.execute(UNPIN_ALL, [])?;
			let mut pin = tx.prepare_cached(PIN)?;
			for card in &pinned {
				pin.execute(named_params! {
					":product": card.product_id.as_str(),
					":card": card.card_id,
					":title": card.title,
				})?;
			}
			tx.execute(DROP_UNPINNED, [])?;
			Ok(())
		})
		.await?;
		Ok(Self { db })
	}

	/// Every card, pinned first, then in the order the user added them, now
	/// and after each change.
	pub fn observe(&self) -> BoxStream<'static, Result<Vec<PocketEntry>, DbError>> {
		self.db.observe(CARDS, |statement| statement.query_map([], entry))
	}

	/// Adds a card the user approved, with the face they approved it by.
	pub async fn add(
		&self,
		product_id: ProductId,
		card_id: String,
		title: String,
		face: RendererNode,
	) -> Result<(), DbError> {
		self.db
			.write(move |tx| {
				tx.prepare_cached(ADD)?.execute(named_params! {
					":product": product_id.as_str(),
					":card": card_id,
					":title": title,
				})?;
				upsert_face(tx, &product_id, &card_id, &face, current_unix_millis())
			})
			.await
	}

	/// Removes a card unless it is pinned.
	pub async fn remove(
		&self,
		product_id: ProductId,
		card_id: String,
	) -> Result<PocketCardRemoval, DbError> {
		self.db
			.write(move |tx| {
				let params = named_params! { ":product": product_id.as_str(), ":card": card_id };
				let privileged: Option<bool> = tx
					.prepare_cached(PRIVILEGED)?
					.query_row(params, |row| row.get(0))
					.optional()?;
				Ok(match privileged {
					None => PocketCardRemoval::Absent,
					Some(true) => PocketCardRemoval::Privileged,
					Some(false) => {
						tx.prepare_cached(DELETE)?.execute(params)?;
						PocketCardRemoval::Removed
					},
				})
			})
			.await
	}

	/// The newest face kept for a card. One that no longer decodes reads as
	/// none, and the card waits for its product to draw again.
	pub async fn face(
		&self,
		product_id: ProductId,
		card_id: String,
	) -> Result<Option<RendererNode>, DbError> {
		let kept: Option<Vec<u8>> = self
			.db
			.read(move |conn| {
				let params = named_params! { ":product": product_id.as_str(), ":card": card_id };
				Ok(conn.prepare_cached(FACE)?.query_row(params, |row| row.get(0)).optional()?)
			})
			.await?;
		Ok(kept.and_then(|bytes| {
			decode_renderer_node(bytes)
				.inspect_err(|error| tracing::warn!(%error, "a kept Pocket face no longer decodes"))
				.ok()
		}))
	}

	/// Keeps `face`, drawn at `drawn_at` unix milliseconds, as the card's
	/// newest, if the card is in the Pocket and no later face is kept.
	pub async fn keep_face(
		&self,
		product_id: ProductId,
		card_id: String,
		face: RendererNode,
		drawn_at: u64,
	) -> Result<(), DbError> {
		self.db
			.write(move |tx| upsert_face(tx, &product_id, &card_id, &face, drawn_at))
			.await
	}

	/// Keeps the newest of `faces`, each with the unix milliseconds it was
	/// drawn at, as the card's face: at once, then at most every
	/// [`FACE_WRITE_INTERVAL`], and once more after `faces` ends.
	pub async fn keep_faces(
		&self,
		product_id: ProductId,
		card_id: String,
		mut faces: UnboundedReceiver<(RendererNode, u64)>,
		timer: Arc<dyn Timer>,
	) {
		while let Some(mut drawn) = faces.next().await {
			while let Ok(newer) = faces.try_recv() {
				drawn = newer;
			}
			let (face, drawn_at) = drawn;
			if let Err(error) =
				self.keep_face(product_id.clone(), card_id.clone(), face, drawn_at).await
			{
				tracing::warn!(%error, card_id, "a Pocket face was not kept");
			}
			timer.sleep(FACE_WRITE_INTERVAL).await;
		}
	}
}

#[async_trait]
impl PocketPlatform for PocketCards {
	fn subscribe_pocket_cards(
		&self,
		product: &ProductContext,
	) -> BoxStream<'static, Result<HostPocketListSubscribeItem, GenericError>> {
		let product_id = product.product_id.as_str().to_owned();
		self.db
			.observe(PRODUCT_CARDS, move |statement| {
				statement.query_map(named_params! { ":product": product_id }, |row| {
					Ok(PocketCard {
						card_id: row.get("card_id")?,
						privileged: row.get("privileged")?,
					})
				})
			})
			.map(|cards| {
				cards
					.map(|cards| HostPocketListSubscribeItem { cards })
					.map_err(|error| GenericError { reason: error.to_string() })
			})
			.boxed()
	}

	async fn remove_pocket_card(
		&self,
		product: &ProductContext,
		request: HostPocketRemoveCardRequest,
	) -> Result<(), HostPocketRemoveCardError> {
		match self.remove(product.product_id.clone(), request.card_id).await {
			Ok(PocketCardRemoval::Privileged) => Err(HostPocketRemoveCardError::Privileged),
			Ok(PocketCardRemoval::Removed | PocketCardRemoval::Absent) => Ok(()),
			Err(error) => Err(HostPocketRemoveCardError::Unknown { reason: error.to_string() }),
		}
	}
}

const CARDS: &str = "SELECT product_id, card_id, title, privileged FROM pocket_card
	ORDER BY privileged DESC, added_at, rowid";

const PRODUCT_CARDS: &str = "SELECT card_id, privileged FROM pocket_card
	WHERE product_id = :product ORDER BY privileged DESC, added_at, rowid";

const UNPIN_ALL: &str = "UPDATE pocket_card SET privileged = 0 WHERE privileged = 1";

const PIN: &str = "INSERT INTO pocket_card (product_id, card_id, title, privileged)
	VALUES (:product, :card, :title, 1)
	ON CONFLICT (product_id, card_id) DO UPDATE SET title = excluded.title, privileged = 1";

const DROP_UNPINNED: &str = "DELETE FROM pocket_card WHERE privileged = 0 AND added_at IS NULL";

const ADD: &str = "INSERT INTO pocket_card (product_id, card_id, title, added_at)
	VALUES (:product, :card, :title, unixepoch())
	ON CONFLICT (product_id, card_id) DO UPDATE SET title = excluded.title,
		added_at = COALESCE(added_at, excluded.added_at)";

const PRIVILEGED: &str =
	"SELECT privileged FROM pocket_card WHERE product_id = :product AND card_id = :card";

const DELETE: &str = "DELETE FROM pocket_card WHERE product_id = :product AND card_id = :card";

const FACE: &str =
	"SELECT face FROM pocket_card_face WHERE product_id = :product AND card_id = :card";

const UPSERT_FACE: &str = "INSERT INTO pocket_card_face (product_id, card_id, face, drawn_at)
	SELECT :product, :card, :face, :drawn_at WHERE EXISTS
		(SELECT 1 FROM pocket_card WHERE product_id = :product AND card_id = :card)
	ON CONFLICT (product_id, card_id) DO UPDATE SET face = excluded.face, drawn_at = excluded.drawn_at
		WHERE excluded.drawn_at >= pocket_card_face.drawn_at";

fn upsert_face(
	tx: &Transaction<'_>,
	product_id: &ProductId,
	card_id: &str,
	face: &RendererNode,
	drawn_at: u64,
) -> Result<(), DbError> {
	tx.prepare_cached(UPSERT_FACE)?.execute(named_params! {
		":product": product_id.as_str(),
		":card": card_id,
		":face": face.encode(),
		":drawn_at": drawn_at,
	})?;
	Ok(())
}

fn entry(row: &Row<'_>) -> rusqlite::Result<PocketEntry> {
	Ok(PocketEntry {
		product_id: row.get("product_id")?,
		card_id: row.get("card_id")?,
		title: row.get("title")?,
		privileged: row.get("privileged")?,
	})
}

#[cfg(test)]
mod tests;
