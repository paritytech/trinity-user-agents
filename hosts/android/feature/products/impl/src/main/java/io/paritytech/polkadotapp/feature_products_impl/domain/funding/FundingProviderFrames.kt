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

/** The screens a session's provider asks to show, which the core holds until the app opens them. */
interface FundingProviderFrames {
    /** What the providers ask of the app's screens. One consumer reads it; events wait until it does. Never ends. */
    val events: Flow<FundingFrameEvent>

    /**
     * Lets the provider of session [intentId] show the screen it asked for, once the session is on screen or the user
     * tapped the notice. The screen then comes as [FundingFrameEvent.Opened]; answers whether one was waiting.
     */
    suspend fun open(intentId: FundingIntentId): Result<Boolean>

    /** Whether the provider of session [intentId] waits for its screen to be opened, so the notice can show again. */
    suspend fun isWaiting(intentId: FundingIntentId): Result<Boolean>

    /** Answers the provider's screen [request], once it is gone. */
    fun answer(request: FundingFrameRequest, outcome: FundingFrameOutcome)
}

sealed interface FundingFrameEvent {
    /** Provider [providerId] wants to show one of its screens for session [intentId], held until the app opens it. */
    data class Requested(val intentId: FundingIntentId, val providerId: ProductId) : FundingFrameEvent

    /** The core presents a provider's screen now, answered through [FundingProviderFrames.answer]. */
    data class Opened(val request: FundingFrameRequest) : FundingFrameEvent

    /** The provider's screens for a session are done: take them down and show the session in their place. */
    data class Closed(val intentId: FundingIntentId) : FundingFrameEvent
}

/** A provider's screen the core presents for session [intentId], showing [target]. */
data class FundingFrameRequest(
    val id: FundingFrameRequestId,
    val intentId: FundingIntentId,
    val providerId: ProductId,
    val target: FundingFrameTarget,
)

/** Serves the core's provider-screen callbacks, which reach it through the funding bridge. */
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

    /** Provider [providerId] asks to show a screen for session [intentId]. */
    fun requested(providerId: ProductId, intentId: FundingIntentId) {
        eventQueue.trySend(FundingFrameEvent.Requested(intentId, providerId))
    }

    /** A provider's screen the core presents now, once the app opened it, answered once the app takes it down. */
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

    /** The provider is done with its screens for session [intentId]: each one still open answers `Closed`. */
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
