//! Which transactions a submission watch owns, so a recovery pass skips them
//! and each transaction has one writer at a time.

use std::collections::{HashMap, HashSet};

use parking_lot::Mutex;
use subxt::utils::H256;

use super::model::DurableTxId;

/// The attempts submission watches currently own.
///
/// Held in memory only: after a crash nothing is owned, which is right,
/// because no watch survived it. Ownership is one-shot per attempt, so a late
/// event from a released watch can never take a transaction back.
#[derive(Default)]
pub struct Ownership {
    state: Mutex<OwnershipState>,
}

#[derive(Default)]
struct OwnershipState {
    owned: HashMap<DurableTxId, H256>,
    released: HashSet<(DurableTxId, H256)>,
}

impl Ownership {
    /// Takes ownership of attempt `tx_hash` of `id`. Refused for an attempt
    /// that was already released.
    pub fn acquire(&self, id: DurableTxId, tx_hash: H256) -> bool {
        let mut state = self.state.lock();
        if state.released.contains(&(id, tx_hash)) {
            return false;
        }
        state.owned.insert(id, tx_hash);
        true
    }

    /// Releases attempt `tx_hash` of `id` for good.
    pub fn release(&self, id: DurableTxId, tx_hash: H256) {
        let mut state = self.state.lock();
        if state.owned.get(&id) == Some(&tx_hash) {
            state.owned.remove(&id);
        }
        state.released.insert((id, tx_hash));
    }

    /// Gives up attempts whose rows rolled back. Unlike [`Self::release`]
    /// they can be owned again: a rolled-back id is handed out anew.
    pub fn abandon_all(&self, attempts: &[(DurableTxId, H256)]) {
        let mut state = self.state.lock();
        for (id, tx_hash) in attempts {
            if state.owned.get(id) == Some(tx_hash) {
                state.owned.remove(id);
            }
        }
    }

    /// Whether a watch owns `id`.
    pub fn is_owned(&self, id: DurableTxId) -> bool {
        self.state.lock().owned.contains_key(&id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: DurableTxId = DurableTxId(1);

    /// Android: RegistrationScenariosTest `ownership is one-shot and never taken back`.
    #[test]
    fn a_released_attempt_is_never_owned_again() {
        let ownership = Ownership::default();
        let hash = H256::repeat_byte(1);
        assert!(ownership.acquire(ID, hash));
        ownership.release(ID, hash);

        assert_eq!(
            (ownership.acquire(ID, hash), ownership.is_owned(ID)),
            (false, false)
        );
    }

    /// iOS: SubmissionWatcherTests `a rebuilt transaction is owned afresh`.
    #[test]
    fn a_new_attempt_of_the_same_transaction_is_owned_afresh() {
        let ownership = Ownership::default();
        ownership.acquire(ID, H256::repeat_byte(1));
        ownership.release(ID, H256::repeat_byte(1));

        assert_eq!(
            (
                ownership.acquire(ID, H256::repeat_byte(2)),
                ownership.is_owned(ID)
            ),
            (true, true)
        );
    }

    #[test]
    fn releasing_a_stale_attempt_leaves_the_current_one_owned() {
        let ownership = Ownership::default();
        ownership.acquire(ID, H256::repeat_byte(2));

        ownership.release(ID, H256::repeat_byte(1));

        assert!(ownership.is_owned(ID));
    }
}
