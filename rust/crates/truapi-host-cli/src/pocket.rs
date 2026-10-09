//! In-memory Pocket host for the CLI.
//!
//! Cards are seeded from `TRUAPI_POCKET_CARDS` and live for the length of the
//! process: this exists to make a Pocket product runnable headlessly, not to be
//! a card store.
//!
//! Every removal this host is asked for is appended to the transcript named by
//! `TRUAPI_POCKET_LOG`, one JSON object per line, so a battery can assert what
//! the host actually did rather than only what the product was told.

use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use futures::StreamExt;
use futures::channel::mpsc;
use futures::stream::{self, BoxStream};
use truapi::latest::{
    GenericError, HostPocketListSubscribeItem, HostPocketRemoveCardError,
    HostPocketRemoveCardRequest,
};
use truapi::platform::{PocketPlatform, ProductContext, async_trait};
use truapi::v01;

/// Cards and list subscribers for one product.
#[derive(Default)]
struct ProductCards {
    /// Card id to whether this host pins it.
    cards: BTreeMap<String, bool>,
    /// Live card-list subscribers for this product.
    subscribers: Vec<mpsc::UnboundedSender<HostPocketListSubscribeItem>>,
}

/// A Pocket host that keeps everything in memory.
///
/// A card id is product-local, so each product gets its own collection seeded
/// from the same spec. Two products sharing one map would let either observe
/// and remove the other's cards after a `/product` switch.
pub struct CliPocketHost {
    seed: BTreeMap<String, bool>,
    products: Mutex<BTreeMap<String, ProductCards>>,
    transcript: Option<PathBuf>,
}

/// Parse `loyalty,humanity:privileged` into card ids and their pinned flags.
fn parse_cards(spec: &str) -> BTreeMap<String, bool> {
    spec.split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(|entry| {
            match entry.split_once(':') {
                Some((card_id, "privileged")) => (card_id.trim().to_string(), true),
                _ => (entry.to_string(), false),
            }
        })
        .collect()
}

impl CliPocketHost {
    /// Build a Pocket host from `TRUAPI_POCKET_CARDS`, recording to
    /// `TRUAPI_POCKET_LOG` when that names a path. No card spec means this host
    /// has no Pocket surface at all.
    pub fn from_env() -> Option<Arc<Self>> {
        let spec = std::env::var("TRUAPI_POCKET_CARDS").ok()?;
        Some(Self::new(
            parse_cards(&spec),
            std::env::var_os("TRUAPI_POCKET_LOG").map(PathBuf::from),
        ))
    }

    /// Build a Pocket host holding `cards`. The transcript is truncated at
    /// startup so a run never reads an earlier run's removals as its own.
    fn new(cards: BTreeMap<String, bool>, transcript: Option<PathBuf>) -> Arc<Self> {
        if let Some(path) = transcript.as_ref()
            && let Err(error) = std::fs::write(path, b"")
        {
            tracing::warn!(?path, %error, "pocket transcript could not be truncated");
        }
        Arc::new(Self {
            seed: cards,
            products: Mutex::new(BTreeMap::new()),
            transcript,
        })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, BTreeMap<String, ProductCards>> {
        self.products
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// This product's collection, seeded on first use so every product starts
    /// from the same spec and diverges from there.
    fn entry<'a>(
        &self,
        products: &'a mut BTreeMap<String, ProductCards>,
        product: &ProductContext,
    ) -> &'a mut ProductCards {
        products
            .entry(product.product_id.clone())
            .or_insert_with(|| {
                ProductCards {
                    cards: self.seed.clone(),
                    subscribers: Vec::new(),
                }
            })
    }

    /// Current card list, in card-id order so a replacement that changes
    /// nothing is byte-identical to the one before it.
    fn card_list(state: &ProductCards) -> HostPocketListSubscribeItem {
        HostPocketListSubscribeItem {
            cards: state
                .cards
                .iter()
                .map(|(card_id, privileged)| {
                    v01::PocketCard {
                        card_id: card_id.clone(),
                        privileged: *privileged,
                    }
                })
                .collect(),
        }
    }

    /// Send the current list to every live subscriber, dropping closed ones.
    fn republish(state: &mut ProductCards) {
        let item = Self::card_list(state);
        state
            .subscribers
            .retain(|subscriber| subscriber.unbounded_send(item.clone()).is_ok());
    }

    /// Append one line to the transcript, if one is configured.
    ///
    /// The line and its terminator go out in a single write: several writes
    /// would let a concurrent connection's line land in the middle of this
    /// one.
    fn record(&self, line: serde_json::Value) {
        let Some(path) = self.transcript.as_ref() else {
            return;
        };
        let appended = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .and_then(|mut file| file.write_all(format!("{line}\n").as_bytes()));
        if let Err(error) = appended {
            tracing::warn!(?path, %error, "pocket transcript could not be appended to");
        }
    }
}

#[async_trait]
impl PocketPlatform for CliPocketHost {
    fn subscribe_pocket_cards(
        &self,
        product: &ProductContext,
    ) -> BoxStream<'static, Result<HostPocketListSubscribeItem, GenericError>> {
        let mut products = self.lock();
        let state = self.entry(&mut products, product);
        let snapshot = Self::card_list(state);
        let (sender, receiver) = mpsc::unbounded();
        state.subscribers.push(sender);
        // The snapshot first, then every replacement, so a product that
        // subscribes before removing a card still sees the removal.
        stream::once(async move { Ok(snapshot) })
            .chain(receiver.map(Ok))
            .boxed()
    }

    async fn remove_pocket_card(
        &self,
        product: &ProductContext,
        request: HostPocketRemoveCardRequest,
    ) -> Result<(), HostPocketRemoveCardError> {
        let mut products = self.lock();
        let state = self.entry(&mut products, product);
        match state.cards.get(&request.card_id) {
            Some(true) => {
                drop(products);
                self.record(serde_json::json!({
                    "kind": "remove_refused",
                    "cardId": request.card_id,
                }));
                Err(HostPocketRemoveCardError::Privileged)
            }
            // A card this host does not hold is already removed.
            None => {
                drop(products);
                self.record(serde_json::json!({
                    "kind": "remove_absent",
                    "cardId": request.card_id,
                }));
                Ok(())
            }
            Some(false) => {
                state.cards.remove(&request.card_id);
                Self::republish(state);
                drop(products);
                self.record(serde_json::json!({
                    "kind": "removed",
                    "cardId": request.card_id,
                }));
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::read_to_string;

    fn product() -> ProductContext {
        ProductContext::new("pocket.dot".to_string()).expect("valid product id")
    }

    fn other_product() -> ProductContext {
        ProductContext::new("other.dot".to_string()).expect("valid product id")
    }

    /// A card id is product-local, so one product must not see or remove
    /// another's cards. The CLI serves every product from one process, and a
    /// `/product` switch reuses the same host.
    #[test]
    fn one_product_cannot_see_or_remove_another_products_cards() {
        let host = CliPocketHost::new(BTreeMap::from([("loyalty".to_string(), false)]), None);
        let mine = product();
        let theirs = other_product();

        let removed = futures::executor::block_on(host.remove_pocket_card(
            &mine,
            HostPocketRemoveCardRequest {
                card_id: "loyalty".to_string(),
            },
        ));
        assert!(removed.is_ok());

        let mut theirs_cards = host.subscribe_pocket_cards(&theirs);
        let seen = futures::executor::block_on(theirs_cards.next())
            .expect("the current list arrives on subscribe")
            .expect("no stream error");
        assert_eq!(
            seen.cards,
            vec![v01::PocketCard {
                card_id: "loyalty".to_string(),
                privileged: false,
            }],
            "the other product still holds its own copy"
        );
    }

    fn host(spec: &str) -> (Arc<CliPocketHost>, tempfile::NamedTempFile) {
        let transcript = tempfile::NamedTempFile::new().expect("a temp transcript");
        let host = CliPocketHost::new(parse_cards(spec), Some(transcript.path().to_path_buf()));
        (host, transcript)
    }

    fn remove(host: &CliPocketHost, card_id: &str) -> Result<(), HostPocketRemoveCardError> {
        futures::executor::block_on(host.remove_pocket_card(
            &product(),
            HostPocketRemoveCardRequest {
                card_id: card_id.to_string(),
            },
        ))
    }

    #[test]
    fn the_card_spec_marks_privileged_cards() {
        let cards = parse_cards("loyalty, humanity:privileged ,");

        assert_eq!(cards.get("loyalty"), Some(&false));
        assert_eq!(cards.get("humanity"), Some(&true));
        assert_eq!(cards.len(), 2);
    }

    #[test]
    fn removal_follows_the_protocol_rules_and_republishes_the_list() {
        let (host, transcript) = host("loyalty,humanity:privileged");
        let mut lists = host.subscribe_pocket_cards(&product());
        let first = futures::executor::block_on(lists.next())
            .expect("the snapshot arrives on subscribe")
            .expect("no stream error");
        assert_eq!(first.cards.len(), 2);

        assert!(matches!(
            remove(&host, "humanity"),
            Err(HostPocketRemoveCardError::Privileged)
        ));
        assert!(remove(&host, "absent").is_ok());
        assert!(remove(&host, "loyalty").is_ok());

        let republished = futures::executor::block_on(lists.next())
            .expect("a removal republishes the list")
            .expect("no stream error");
        assert_eq!(republished.cards.len(), 1);
        assert!(republished.cards[0].privileged);

        // What the host itself observed, which is what a battery reads to tell
        // a refusal apart from a removal it never attempted.
        let kinds: Vec<String> = read_to_string(transcript.path())
            .expect("the transcript is readable")
            .lines()
            .map(|line| {
                serde_json::from_str::<serde_json::Value>(line).expect("one json object per line")
                    ["kind"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string()
            })
            .collect();
        assert_eq!(kinds, ["remove_refused", "remove_absent", "removed"]);
    }
}
