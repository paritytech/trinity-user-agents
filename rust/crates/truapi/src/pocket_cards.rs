//! The Pocket collection in the core database: which cards are in the Pocket,
//! and the newest face drawn for each.
//!
//! The host hands over the cards it pins when the runtime starts and adds the
//! cards the user approves. Products read their own cards and may remove them.
//! The faces their renders draw are kept here, so a card has one to show
//! offline and at the next start.

use core::time::Duration;
use std::sync::{
	Arc,
	atomic::{AtomicU64, Ordering},
};

use futures::{
	FutureExt, StreamExt,
	channel::mpsc::UnboundedReceiver,
	future::{Fuse, FusedFuture},
	select,
	stream::BoxStream,
};
use parity_scale_codec::{DecodeLimit, Encode};
use rusqlite::{OptionalExtension, Row, Transaction, named_params, types::Type};
use truapi::latest::{
	GenericError, HostPocketListSubscribeItem, HostPocketRemoveCardError,
	HostPocketRemoveCardRequest, PocketCard, RendererNode,
};

use crate::{
	native::MAX_FACE_DEPTH,
	platform::{
		ChatFieldError, PocketPlatform, ProductContext, ProductId, async_trait,
		normalize_chat_identifier, validate_chat_name,
	},
	store::{Db, DbError},
	time::Timer,
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

impl PinnedPocketCard {
	/// The card with its id and title screened as a product's would be, so a
	/// product can address it.
	pub fn screened(self) -> Result<Self, ChatFieldError> {
		Ok(Self {
			card_id: normalize_chat_identifier("cardId", &self.card_id)?,
			title: validate_chat_name("title", &self.title)?,
			..self
		})
	}
}

/// One card in the Pocket, as the host lists it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct PocketEntry {
	/// Product that backs the card.
	pub product_id: ProductId,
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

/// Why a card was not added.
#[derive(Debug, Clone, thiserror::Error, uniffi::Error)]
pub enum PocketAddError {
	/// The card id or title is not one a product could use.
	#[error("{reason}")]
	Refused {
		/// Which field was refused, and why.
		reason: String,
	},
	/// The core database failed.
	#[error("{reason}")]
	Unavailable {
		/// What failed.
		reason: String,
	},
}

impl From<ChatFieldError> for PocketAddError {
	fn from(error: ChatFieldError) -> Self {
		Self::Refused { reason: error.to_string() }
	}
}

impl From<DbError> for PocketAddError {
	fn from(error: DbError) -> Self {
		Self::Unavailable { reason: error.to_string() }
	}
}

/// Least time between two writes of one card's face. A product redraws at
/// frame rate, and the card needs one recent face, not every one.
const FACE_WRITE_INTERVAL: Duration = Duration::from_secs(2);

/// The Pocket collection over the core database.
#[derive(Clone)]
pub struct PocketCards {
	db: Db,
	/// Orders the faces this process draws. Faces from earlier runs are set
	/// below it on open, so the wall clock never decides which face is newer.
	draws: Arc<AtomicU64>,
}

impl PocketCards {
	/// The collection in `db`, with `pinned` as the host's pins, listed in that
	/// order. A card no longer pinned stays only if the user added it.
	pub async fn open(db: Db, pinned: Vec<PinnedPocketCard>) -> Result<Self, DbError> {
		db.write(move |tx| {
			tx.execute(UNPIN_ALL, [])?;
			let mut pin = tx.prepare_cached(PIN)?;
			for (position, card) in pinned.iter().enumerate() {
				pin.execute(named_params! {
					":product": card.product_id.as_str(),
					":card": card.card_id,
					":title": card.title,
					":pin": position,
				})?;
			}
			tx.execute(DROP_UNPINNED, [])?;
			tx.execute(AGE_FACES, [])?;
			Ok(())
		})
		.await?;
		Ok(Self { db, draws: Arc::new(AtomicU64::new(0)) })
	}

	/// The place of a face drawn now among every face this process keeps.
	pub fn next_draw(&self) -> u64 {
		self.draws.fetch_add(1, Ordering::Relaxed) + 1
	}

	/// Every card, pinned first, then in the order the user added them, now
	/// and after each change.
	pub fn observe(&self) -> BoxStream<'static, Result<Vec<PocketEntry>, DbError>> {
		self.db.observe(CARDS, |statement| statement.query_map([], entry))
	}

	/// Adds a card the user approved, with the face they approved it by. The
	/// id and title are screened as a product's would be, so the product can
	/// address the card.
	pub async fn add(
		&self,
		product_id: ProductId,
		card_id: String,
		title: String,
		face: RendererNode,
	) -> Result<(), PocketAddError> {
		let card_id = normalize_chat_identifier("cardId", &card_id)?;
		let title = validate_chat_name("title", &title)?;
		let draw_order = self.next_draw();
		self.db
			.write(move |tx| {
				tx.prepare_cached(ADD)?.execute(named_params! {
					":product": product_id.as_str(),
					":card": card_id,
					":title": title,
				})?;
				upsert_face(tx, &product_id, &card_id, &face, draw_order)
			})
			.await?;
		Ok(())
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
			RendererNode::decode_all_with_depth_limit(MAX_FACE_DEPTH, &mut bytes.as_slice())
				.inspect_err(|error| tracing::warn!(%error, "a kept Pocket face no longer decodes"))
				.ok()
		}))
	}

	/// Keeps `face`, drawn at [`Self::next_draw`] `draw_order`, as the card's
	/// newest, if the card is in the Pocket and no later face is kept.
	pub async fn keep_face(
		&self,
		product_id: ProductId,
		card_id: String,
		face: RendererNode,
		draw_order: u64,
	) -> Result<(), DbError> {
		self.db
			.write(move |tx| upsert_face(tx, &product_id, &card_id, &face, draw_order))
			.await
	}

	/// Keeps the newest of `faces`, each with its [`Self::next_draw`] order,
	/// as the card's face: at once, then at most every
	/// [`FACE_WRITE_INTERVAL`], and at once again when `faces` ends.
	pub async fn keep_faces(
		&self,
		product_id: ProductId,
		card_id: String,
		mut faces: UnboundedReceiver<(RendererNode, u64)>,
		timer: Arc<dyn Timer>,
	) {
		let mut newest = None;
		let mut interval = Fuse::terminated();
		loop {
			select! {
				drawn = faces.next() => match drawn {
					Some(drawn) => newest = Some(drawn),
					None => break,
				},
				() = interval => {},
			}
			if !interval.is_terminated() {
				continue;
			}
			// A face already queued is newer than the one in hand.
			while let Ok(newer) = faces.try_recv() {
				newest = Some(newer);
			}
			if let Some(drawn) = newest.take() {
				self.keep_newest(&product_id, &card_id, drawn).await;
				interval = timer.sleep(FACE_WRITE_INTERVAL).fuse();
			}
		}
		if let Some(drawn) = newest {
			self.keep_newest(&product_id, &card_id, drawn).await;
		}
	}

	async fn keep_newest(
		&self,
		product_id: &ProductId,
		card_id: &str,
		(face, draw_order): (RendererNode, u64),
	) {
		if let Err(error) =
			self.keep_face(product_id.clone(), card_id.to_owned(), face, draw_order).await
		{
			tracing::warn!(%error, card_id, "a Pocket face was not kept");
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

const CARDS: &str = "SELECT product_id, card_id, title, pin IS NOT NULL AS privileged
	FROM pocket_card ORDER BY pin IS NULL, pin, added_order";

const PRODUCT_CARDS: &str = "SELECT card_id, pin IS NOT NULL AS privileged FROM pocket_card
	WHERE product_id = :product ORDER BY pin IS NULL, pin, added_order";

const UNPIN_ALL: &str = "UPDATE pocket_card SET pin = NULL WHERE pin IS NOT NULL";

const PIN: &str = "INSERT INTO pocket_card (product_id, card_id, title, pin)
	VALUES (:product, :card, :title, :pin)
	ON CONFLICT (product_id, card_id) DO UPDATE SET title = excluded.title, pin = excluded.pin";

const DROP_UNPINNED: &str = "DELETE FROM pocket_card WHERE pin IS NULL AND added_order IS NULL";

const AGE_FACES: &str = "UPDATE pocket_card_face SET draw_order = 0";

const ADD: &str = "INSERT INTO pocket_card (product_id, card_id, title, added_order)
	VALUES (:product, :card, :title, (SELECT COALESCE(MAX(added_order), 0) + 1 FROM pocket_card))
	ON CONFLICT (product_id, card_id) DO UPDATE
		SET title = CASE WHEN pin IS NULL THEN excluded.title ELSE title END,
		added_order = COALESCE(added_order, excluded.added_order)";

const PRIVILEGED: &str =
	"SELECT pin IS NOT NULL FROM pocket_card WHERE product_id = :product AND card_id = :card";

const DELETE: &str = "DELETE FROM pocket_card WHERE product_id = :product AND card_id = :card";

const FACE: &str =
	"SELECT face FROM pocket_card_face WHERE product_id = :product AND card_id = :card";

const UPSERT_FACE: &str = "INSERT INTO pocket_card_face (product_id, card_id, face, draw_order)
	SELECT :product, :card, :face, :draw_order WHERE EXISTS
		(SELECT 1 FROM pocket_card WHERE product_id = :product AND card_id = :card)
	ON CONFLICT (product_id, card_id) DO UPDATE SET face = excluded.face, draw_order = excluded.draw_order
		WHERE excluded.draw_order > pocket_card_face.draw_order";

fn upsert_face(
	tx: &Transaction<'_>,
	product_id: &ProductId,
	card_id: &str,
	face: &RendererNode,
	draw_order: u64,
) -> Result<(), DbError> {
	tx.prepare_cached(UPSERT_FACE)?.execute(named_params! {
		":product": product_id.as_str(),
		":card": card_id,
		":face": face.encode(),
		":draw_order": draw_order,
	})?;
	Ok(())
}

fn entry(row: &Row<'_>) -> rusqlite::Result<PocketEntry> {
	let product_id: String = row.get("product_id")?;
	Ok(PocketEntry {
		product_id: product_id.parse().map_err(|error| {
			rusqlite::Error::FromSqlConversionFailure(0, Type::Text, Box::new(error))
		})?,
		card_id: row.get("card_id")?,
		title: row.get("title")?,
		privileged: row.get("privileged")?,
	})
}

#[cfg(test)]
mod tests;
