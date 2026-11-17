package io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model

/**
 * Coins handed out of the wallet, provisionally until [commit].
 *
 * Commit before whatever carries their keys is made durable: for a chat payment, inside the transaction that
 * writes the message, so no peer can hold keys this device does not count as handed out.
 */
interface CoinageHandoffCommit {
    suspend fun commit(): Result<Unit>

    /** Takes the coins back, for a payment whose keys never left. A committed handout is not touched. */
    suspend fun release(): Result<Unit>
}
