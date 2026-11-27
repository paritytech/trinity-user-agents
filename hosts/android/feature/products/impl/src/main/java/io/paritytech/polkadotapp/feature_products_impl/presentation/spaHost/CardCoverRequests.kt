package io.paritytech.polkadotapp.feature_products_impl.presentation.spaHost

import io.paritytech.polkadotapp.feature_products_api.presentation.spaHost.CardCoverAnswer
import io.paritytech.polkadotapp.feature_products_api.presentation.spaHost.CardCoverRequest
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.withTimeoutOrNull
import uniffi.truapi.ExpandedCardCoverOutcome
import kotlin.time.Duration.Companion.seconds

/** What the product under an expanded card asks of that card. */
fun interface ExpandedCardCover {
    suspend fun setCovered(covered: Boolean): ExpandedCardCoverOutcome
}

/**
 * Carries a product's cover requests to the screen drawing its card, which alone knows whether the
 * user is dragging. With no screen watching, the card is closed and nothing is moved.
 */
internal class CardCoverRequests : ExpandedCardCover {
    private val outgoing = MutableSharedFlow<CardCoverRequest>()

    val requests: Flow<CardCoverRequest> get() = outgoing

    override suspend fun setCovered(covered: Boolean): ExpandedCardCoverOutcome {
        if (outgoing.subscriptionCount.value == 0) return ExpandedCardCoverOutcome.NOT_PRESENTED
        val reply = CompletableDeferred<CardCoverAnswer>()
        val answer = try {
            withTimeoutOrNull(REPLY_TIMEOUT) {
                outgoing.emit(CardCoverRequest(covered, reply))
                reply.await()
            }
        } finally {
            reply.cancel()
        }
        return when (answer) {
            CardCoverAnswer.APPLIED -> ExpandedCardCoverOutcome.APPLIED
            CardCoverAnswer.USER_MOVING -> ExpandedCardCoverOutcome.USER_MOVING
            CardCoverAnswer.NOT_PRESENTED, null -> ExpandedCardCoverOutcome.NOT_PRESENTED
        }
    }

    private companion object {
        val REPLY_TIMEOUT = 1.seconds
    }
}
