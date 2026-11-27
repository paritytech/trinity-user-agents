package io.paritytech.polkadotapp.feature_products_impl.domain.funding

import io.paritytech.polkadotapp.common.utils.logFailure
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.receiveAsFlow
import uniffi.truapi.FundingFrameOutcome
import uniffi.truapi.FundingFrameTarget
import uniffi.truapi.ProductId
import java.util.UUID
import javax.inject.Inject
import javax.inject.Singleton

interface FundingProviderFrames {
    // Single consumer; events wait until it reads. Never ends.
    val events: Flow<FundingFrameEvent>

    suspend fun open(intentId: FundingIntentId): Result<Boolean>

    suspend fun isWaiting(intentId: FundingIntentId): Result<Boolean>

    fun answer(request: FundingFrameRequest, outcome: FundingFrameOutcome)
}

sealed interface FundingFrameEvent {
    data class Requested(val intentId: FundingIntentId, val providerId: ProductId) : FundingFrameEvent

    data class Opened(val request: FundingFrameRequest) : FundingFrameEvent

    data class Closed(val intentId: FundingIntentId) : FundingFrameEvent
}

data class FundingFrameRequest(
    val id: FundingFrameRequestId,
    val intentId: FundingIntentId,
    val providerId: ProductId,
    val target: FundingFrameTarget,
)

@Singleton
class RealFundingProviderFrames @Inject constructor(
    private val core: FundingCore,
) : FundingProviderFrames {
    private val lock = Any()
    private val frames = mutableMapOf<FundingFrameRequestId, PendingFrame>()
    private val eventQueue = Channel<FundingFrameEvent>(Channel.UNLIMITED)

    override val events: Flow<FundingFrameEvent> = eventQueue.receiveAsFlow()

    override suspend fun open(intentId: FundingIntentId): Result<Boolean> =
        core.openFrame(intentId).logFailure("Funding provider screen could not open")

    override suspend fun isWaiting(intentId: FundingIntentId): Result<Boolean> =
        core.frameWaiting(intentId).logFailure("Funding provider screen read failed")

    override fun answer(request: FundingFrameRequest, outcome: FundingFrameOutcome) {
        val pending = synchronized(lock) { frames.remove(request.id) }
        pending?.answer?.complete(outcome)
    }

    fun requested(providerId: ProductId, intentId: FundingIntentId) {
        eventQueue.trySend(FundingFrameEvent.Requested(intentId, providerId))
    }

    suspend fun present(providerId: ProductId, intentId: FundingIntentId, target: FundingFrameTarget): FundingFrameOutcome {
        val request = FundingFrameRequest(
            id = FundingFrameRequestId(UUID.randomUUID().toString()),
            intentId = intentId,
            providerId = providerId,
            target = target,
        )
        val frame = PendingFrame(request)
        synchronized(lock) { frames[request.id] = frame }
        try {
            eventQueue.trySend(FundingFrameEvent.Opened(request))
            return frame.answer.await()
        } catch (cancellation: CancellationException) {
            synchronized(lock) { frames.remove(request.id) }
            eventQueue.trySend(FundingFrameEvent.Closed(intentId))
            throw cancellation
        }
    }

    fun close(providerId: ProductId, intentId: FundingIntentId) {
        val closed = synchronized(lock) {
            frames.values
                .filter { it.request.intentId == intentId && it.request.providerId == providerId }
                .onEach { frames.remove(it.request.id) }
        }
        closed.forEach { it.answer.complete(FundingFrameOutcome.Closed) }
        eventQueue.trySend(FundingFrameEvent.Closed(intentId))
    }

    private class PendingFrame(val request: FundingFrameRequest) {
        val answer = CompletableDeferred<FundingFrameOutcome>()
    }
}
