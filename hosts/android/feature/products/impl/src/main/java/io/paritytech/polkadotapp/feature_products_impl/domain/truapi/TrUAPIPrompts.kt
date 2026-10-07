package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeoutOrNull

/** One host screen a product is waiting on: what it shows, and the answer it gives. */
class TrUAPIPrompt<Q, A>(val question: Q, private val unanswered: A) {
    private val answer = CompletableDeferred<A>()
    private val shown = CompletableDeferred<Unit>()

    /** Called by the screen once it holds this prompt. */
    fun markShown() {
        shown.complete(Unit)
    }

    /** The first answer wins. */
    fun answer(value: A) {
        answer.complete(value)
    }

    /** Also the answer for a screen that goes away unanswered. */
    fun dismiss() = answer(unanswered)

    /** True once the core has its answer, so a screen that appears late closes. */
    val isAnswered get() = answer.isCompleted

    internal suspend fun awaitShown(timeoutMs: Long) = withTimeoutOrNull(timeoutMs) { shown.await() } != null

    internal suspend fun await(): A = answer.await()
}

/**
 * Shows one kind of host screen at a time and waits for its answer. The screen
 * reads what to show from [current].
 */
abstract class TrUAPIPrompts<Q, A>(private val unanswered: A) {
    private val oneAtATime = Mutex()

    /** The prompt the screen is showing, or the last one shown. */
    @Volatile
    var current: TrUAPIPrompt<Q, A>? = null
        private set

    protected abstract suspend fun open()

    protected abstract suspend fun close()

    suspend fun ask(question: Q): A = oneAtATime.withLock {
        val prompt = TrUAPIPrompt(question, unanswered).also { current = it }
        try {
            open()
            // Navigation can fail silently, and then no screen would ever answer.
            check(prompt.awaitShown(SHOWN_TIMEOUT_MS)) { "the host screen did not appear" }
            prompt.await()
        } catch (failure: Throwable) {
            prompt.dismiss()
            withContext(NonCancellable) { close() }
            throw failure
        }
    }

    private companion object {
        const val SHOWN_TIMEOUT_MS = 10_000L
    }
}
