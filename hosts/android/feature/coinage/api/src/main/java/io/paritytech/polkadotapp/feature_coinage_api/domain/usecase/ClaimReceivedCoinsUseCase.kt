package io.paritytech.polkadotapp.feature_coinage_api.domain.usecase

import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinPrivateKey
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageTransferDetection
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageOperationGroupId
import kotlinx.coroutines.flow.Flow
import kotlin.time.ExperimentalTime
import kotlin.time.Instant

/**
 * Moves coins a peer handed us into our own coin set.
 *
 * The claims themselves are ordinary coinage transactions, so their outcome is the ledger's to decide: this
 * only detects what the keys control, submits, and reports what the ledger then says.
 */
interface ClaimReceivedCoinsUseCase {
    /**
     * Reports [CoinageTransferDetection.Transferred] as soon as a claim is included in a block, and keeps
     * emitting until the claim is over — so a fork that takes an inclusion away is reported too, as a return
     * to [CoinageTransferDetection.Detected].
     *
     * Each coin's claim is registered once, as soon as the chain shows the coin, and a claim that fails is built
     * again by its submission policy into the same coin until [retryUntil] allows no more. The flow completes
     * once every coin has a claim that can no longer change, or when [retryUntil] has passed on coins that never
     * appeared — and never before at least one attempt has been made and the chain has answered at least once, so
     * a message first seen after its window has closed is still tried rather than abandoned. Pass
     * [Instant.DISTANT_FUTURE] to never give up.
     *
     * Because it ends only when nothing further will be attempted, completion is what tells a caller the
     * payment is finished. No status emitted along the way means that, a partial
     * [CoinageTransferDetection.Transferred] least of all.
     *
     * [groupId] must be derived from something stable about the payment — a chat message's id, the coin keys
     * themselves — because a second call with the same id rejoins the claims already submitted instead of
     * submitting them again. That is what makes a retry after process death safe.
     */
    @OptIn(ExperimentalTime::class)
    fun claim(
        coinKeys: List<CoinPrivateKey>,
        groupId: CoinageOperationGroupId,
        retryUntil: Instant,
    ): Flow<CoinageTransferDetection>
}
