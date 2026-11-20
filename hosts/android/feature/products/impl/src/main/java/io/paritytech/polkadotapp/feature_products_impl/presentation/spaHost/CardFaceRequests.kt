package io.paritytech.polkadotapp.feature_products_impl.presentation.spaHost

import io.paritytech.polkadotapp.feature_products_api.presentation.spaHost.ExpandedCardRequest
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.withTimeoutOrNull
import uniffi.truapi.ExpandedCardFaceOutcome
import uniffi.truapi.ExpandedCardTurnOutcome
import uniffi.truapi.PocketCardSide
import kotlin.time.Duration.Companion.seconds

/** What the product under an expanded card asks of that card. */
interface ExpandedCardFace {
    suspend fun setFaceShown(shown: Boolean): ExpandedCardFaceOutcome

    suspend fun setBackShown(shown: Boolean): ExpandedCardTurnOutcome
}

/**
 * Carries a product's face requests to the screen drawing its card, which alone knows whether the
 * user is dragging. With no screen watching, the card is closed and nothing is moved.
 */
internal class CardFaceRequests : ExpandedCardFace {
    private val outgoing = MutableSharedFlow<ExpandedCardRequest<*>>()

    val requests: Flow<ExpandedCardRequest<*>> get() = outgoing

    override suspend fun setFaceShown(shown: Boolean) = send(ExpandedCardRequest.Face(shown))

    override suspend fun setBackShown(shown: Boolean) =
        send(ExpandedCardRequest.Turn(if (shown) PocketCardSide.BACK else PocketCardSide.FRONT))

    private suspend fun <Outcome : Any> send(request: ExpandedCardRequest<Outcome>): Outcome {
        if (outgoing.subscriptionCount.value == 0) return request.notPresented
        val answer = try {
            withTimeoutOrNull(REPLY_TIMEOUT) {
                outgoing.emit(request)
                request.reply.await()
            }
        } finally {
            request.reply.cancel()
        }
        return answer ?: request.notPresented
    }

    private companion object {
        val REPLY_TIMEOUT = 1.seconds
    }
}
