package io.paritytech.polkadotapp.feature_coinage_api.domain.service

import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinPrivateKey
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinageExportId
import io.paritytech.polkadotapp.feature_coinage_api.domain.transaction.model.CoinageOperationGroupId
import kotlin.time.ExperimentalTime
import kotlin.time.Instant

/**
 * The payments the app's chat still follows, which the wallet hands to the core with the rest of the wallet the
 * app ran before, so the core settles them.
 */
interface CoinageHandOverPayments {
    suspend fun payments(): HandedOverPayments
}

data class HandedOverPayments(
    /** Payments we sent whose coins the peer may not have taken yet. */
    val outgoing: List<HandedOverOutgoingPayment>,
    /** Payments we received and have not claimed yet. */
    val incoming: List<HandedOverIncomingPayment>,
)

/** A sent payment, followed afterwards under [exportId]. */
data class HandedOverOutgoingPayment(
    val exportId: CoinageExportId,
    val coinKeys: List<CoinPrivateKey>,
)

/** A received payment, claimed under [claimGroupId] until [claimBy]. */
@OptIn(ExperimentalTime::class)
data class HandedOverIncomingPayment(
    val claimGroupId: CoinageOperationGroupId,
    val claimBy: Instant,
    val coinKeys: List<CoinPrivateKey>,
)
