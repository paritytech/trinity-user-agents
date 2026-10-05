//! Stable protected-storage scoping for paired sessions.

use crate::host_logic::session::SsoSessionInfo;

/// Bind a record to both capability session identifiers.
pub fn session_storage_id(session: &SsoSessionInfo) -> String {
    hex::encode(
        [
            session.session_id_own.as_slice(),
            session.session_id_peer.as_slice(),
        ]
        .concat(),
    )
}
