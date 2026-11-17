package io.paritytech.polkadotapp.feature_products_impl.presentation.spaHost

import io.paritytech.polkadotapp.feature_products_api.presentation.spaHost.FaceShownAnswer
import io.paritytech.polkadotapp.feature_products_api.presentation.spaHost.FaceShownRequest
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.withTimeoutOrNull
import uniffi.truapi.ExpandedCardFaceOutcome
import uniffi.truapi.ExpandedCardTurnOutcome
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
    private val outgoing = MutableSharedFlow<FaceShownRequest>()

    val requests: Flow<FaceShownRequest> get() = outgoing

    override suspend fun setFaceShown(shown: Boolean) = when (send(shown, back = false)) {
        FaceShownAnswer.APPLIED -> ExpandedCardFaceOutcome.APPLIED
        FaceShownAnswer.USER_MOVING -> ExpandedCardFaceOutcome.USER_MOVING
        FaceShownAnswer.NOT_PRESENTED, FaceShownAnswer.NO_BACK -> ExpandedCardFaceOutcome.NOT_PRESENTED
    }

    override suspend fun setBackShown(shown: Boolean) = when (send(shown, back = true)) {
        FaceShownAnswer.APPLIED -> ExpandedCardTurnOutcome.APPLIED
        FaceShownAnswer.USER_MOVING -> ExpandedCardTurnOutcome.USER_MOVING
        FaceShownAnswer.NO_BACK -> ExpandedCardTurnOutcome.NO_BACK
        FaceShownAnswer.NOT_PRESENTED -> ExpandedCardTurnOutcome.NOT_PRESENTED
    }

    private suspend fun send(shown: Boolean, back: Boolean): FaceShownAnswer {
        if (outgoing.subscriptionCount.value == 0) return FaceShownAnswer.NOT_PRESENTED
        val reply = CompletableDeferred<FaceShownAnswer>()
        val answer = try {
            withTimeoutOrNull(REPLY_TIMEOUT) {
                outgoing.emit(FaceShownRequest(shown, reply, back))
                reply.await()
            }
        } finally {
            reply.cancel()
        }
        return answer ?: FaceShownAnswer.NOT_PRESENTED
    }

    private companion object {
        val REPLY_TIMEOUT = 1.seconds
    }
}
