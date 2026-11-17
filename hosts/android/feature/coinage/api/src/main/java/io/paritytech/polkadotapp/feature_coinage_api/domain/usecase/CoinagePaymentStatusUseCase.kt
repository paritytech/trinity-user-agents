package io.paritytech.polkadotapp.feature_coinage_api.domain.usecase

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageExportId
import kotlinx.coroutines.flow.Flow

/** What happened to the coins of a payment we handed to a peer. */
interface CoinagePaymentStatusUseCase {
    /**
     * The coins handed out under [exportId], re-emitted whenever one of them changes. Fails when nothing was
     * handed out under it.
     */
    fun subscribeStatuses(exportId: CoinageExportId): Flow<List<CoinagePaymentState>>
}

/** One coin handed to a peer: its on-chain [account], its [value], and what has become of it. */
data class CoinagePaymentState(
    val account: AccountId,
    val value: Balance,
    val status: CoinagePaymentStatus,
)

/**
 * What has become of one coin handed to a peer.
 *
 * Only [Failed] and a finalized [Claimed] are proven; everything else can change with a fork.
 */
sealed interface CoinagePaymentStatus {
    /** On chain: the peer has not taken it yet. */
    data object AwaitingClaim : CoinagePaymentStatus

    /** Not minted yet. */
    data object Detecting : CoinagePaymentStatus

    /** Never minted, or moved back to us. The key controls nothing the peer can take. */
    data object Failed : CoinagePaymentStatus

    /** The peer took it; [finalized] when no fork can undo that. */
    data class Claimed(val finalized: Boolean) : CoinagePaymentStatus
}

/** Whether nothing can change this any more, and so whether a payment may be closed on it. */
val CoinagePaymentStatus.isTerminal: Boolean
    get() = this is CoinagePaymentStatus.Failed || (this is CoinagePaymentStatus.Claimed && finalized)
