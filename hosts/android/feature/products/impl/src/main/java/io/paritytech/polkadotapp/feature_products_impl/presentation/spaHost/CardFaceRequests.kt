package io.paritytech.polkadotapp.feature_products_impl.presentation.spaHost

import io.paritytech.polkadotapp.feature_products_api.presentation.spaHost.FaceShownAnswer
import io.paritytech.polkadotapp.feature_products_api.presentation.spaHost.FaceShownRequest
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.withTimeoutOrNull
import uniffi.truapi.ExpandedCardFaceOutcome
import kotlin.time.Duration.Companion.seconds

/** What the product under an expanded card asks of that card. */
fun interface ExpandedCardFace {
    suspend fun setFaceShown(shown: Boolean): ExpandedCardFaceOutcome
}

/**
 * Carries a product's face requests to the screen drawing its card, which alone knows whether the
 * user is dragging. With no screen watching, the card is closed and nothing is moved.
 */
internal class CardFaceRequests : ExpandedCardFace {
    private val outgoing = MutableSharedFlow<FaceShownRequest>()

    val requests: Flow<FaceShownRequest> get() = outgoing

    override suspend fun setFaceShown(shown: Boolean): ExpandedCardFaceOutcome {
        if (outgoing.subscriptionCount.value == 0) return ExpandedCardFaceOutcome.NOT_PRESENTED
        val answer = withTimeoutOrNull(REPLY_TIMEOUT) {
            val reply = CompletableDeferred<FaceShownAnswer>()
            outgoing.emit(FaceShownRequest(shown, reply))
            reply.await()
        }
        return when (answer) {
            FaceShownAnswer.APPLIED -> ExpandedCardFaceOutcome.APPLIED
            FaceShownAnswer.USER_MOVING -> ExpandedCardFaceOutcome.USER_MOVING
            null -> ExpandedCardFaceOutcome.NOT_PRESENTED
        }
    }

    private companion object {
        val REPLY_TIMEOUT = 1.seconds
    }
}
