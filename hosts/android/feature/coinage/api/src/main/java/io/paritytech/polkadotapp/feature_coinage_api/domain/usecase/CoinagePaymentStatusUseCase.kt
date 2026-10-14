package io.paritytech.polkadotapp.feature_coinage_api.domain.usecase

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.Coin
import kotlinx.coroutines.flow.Flow

/**
 * What happened to a coin whose key we handed to a peer.
 *
 * Reads on-chain presence and consults the minter's status only to disambiguate absence, so the result is
 * identical whichever transfer strategy produced the coins — including the one submitting no extrinsic.
 * Nothing is cached.
 *
 * Keyed by the coin's on-chain account id rather than its derivation index: a caller watching a payment holds
 * the keys it handed over, and the account id is what those derive to. A coin we never minted is absent from
 * the result — the question only means something about our own.
 */
interface CoinagePaymentStatusUseCase {
    fun subscribeStatuses(coins: List<AccountId>): Flow<Map<AccountId, CoinagePaymentState>>
}

/**
 * The whole [coin] is carried, not just its identity: answering this question already joined the row, and a
 * caller reporting how much of a payment landed needs its value. Handing back an identity to be looked up
 * again is the same read twice.
 */
data class CoinagePaymentState(
    val coin: Coin,
    val status: CoinagePaymentStatus,
)

/**
 * What has become of one coin handed to a peer.
 *
 * Only [Failed] and a finalized [Claimed] are proven. Everything else is read at the best head and pairs two
 * facts — the chain's and the ledger's — that were observed independently, so it can be wrong for a moment
 * and must never be the grounds for closing a payment.
 */
sealed interface CoinagePaymentStatus {
    /** Present on chain: the peer has not taken it yet. */
    data object AwaitingClaim : CoinagePaymentStatus

    /** Absent, and its minter has not resolved: it may simply not exist yet. */
    data object Detecting : CoinagePaymentStatus

    /** Absent because it was never minted. The key controls nothing, and nothing can change that. */
    data object Failed : CoinagePaymentStatus

    /**
     * Absent after having existed, so the peer took it.
     *
     * [finalized] separates a guess from a proof. It is true only when the mint finalized *and* the coin is
     * absent from the finalized chain — two facts read at one vantage point, which is what makes them safe
     * to pair. Until then this is inferred from the best head, where a fork can put the coin back and a
     * stale read can claim it was never there.
     */
    data class Claimed(val finalized: Boolean) : CoinagePaymentStatus
}

/** Whether nothing can change this any more, and so whether a payment may be closed on it. */
val CoinagePaymentStatus.isTerminal: Boolean
    get() = this is CoinagePaymentStatus.Failed || (this is CoinagePaymentStatus.Claimed && finalized)
