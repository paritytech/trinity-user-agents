package io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model

/**
 * A handoff that is not yet final: its reservation is either already written or written only by [commit].
 *
 * Hold one from the moment the assets are chosen until whatever carries their keys is durable, then commit.
 * Where that is depends on the transport: for a chat payment it is the message row, so the commit belongs
 * inside the transaction that writes it — a crash in between would otherwise leave a peer holding the keys
 * with no final mark on this device.
 */
interface CoinageHandoffCommit {
    suspend fun commit(): Result<Unit>

    /** Drops any reservation now, for a payment whose keys never left. A committed handoff is not touched. */
    suspend fun release(): Result<Unit>
}
