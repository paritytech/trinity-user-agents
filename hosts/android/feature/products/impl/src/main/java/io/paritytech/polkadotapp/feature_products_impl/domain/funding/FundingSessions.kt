package io.paritytech.polkadotapp.feature_products_impl.domain.funding

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.toBalanceOrNull
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.channels.BufferOverflow
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.conflate
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.mapNotNull
import kotlinx.coroutines.flow.merge
import kotlinx.coroutines.flow.onSubscription
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.flow.transformWhile
import kotlinx.coroutines.withTimeoutOrNull
import timber.log.Timber
import uniffi.truapi.CancelConfirmation
import uniffi.truapi.CancelOutcome
import uniffi.truapi.CancelTerms
import uniffi.truapi.FundingAmountIssue
import uniffi.truapi.FundingDirection
import uniffi.truapi.FundingFailure
import uniffi.truapi.FundingOptions
import uniffi.truapi.FundingPresentOutcome
import uniffi.truapi.FundingProgress
import uniffi.truapi.FundingSelection
import uniffi.truapi.FundingSession
import uniffi.truapi.FundingSessionSummary
import uniffi.truapi.ProductId
import javax.inject.Inject
import javax.inject.Singleton

// The core owns every session: the screens follow its state and report what the user picked.
interface FundingSessions {
    // Newest first, in flight and ended. Never ends.
    fun entries(): Flow<List<FundingEntry>>

    // Completes after the session ended, and without a value once it is gone (dropped, or left before it started).
    fun entryState(intentId: FundingIntentId): Flow<FundingEntry>

    // One consumer; each request waits until it is read. Never ends.
    val presentationRequests: Flow<FundingEntry>

    fun shown(intentId: FundingIntentId)

    suspend fun isAvailable(): Result<Boolean>

    suspend fun resume(): Result<Unit>

    // Answers `null` once the user left the session.
    suspend fun open(direction: FundingDirection, amount: Balance?): Result<FundingIntentId?>

    fun select(selection: FundingSelection, intentId: FundingIntentId)

    suspend fun amountIssue(amount: Balance, spendable: Balance?, intentId: FundingIntentId): Result<FundingAmountIssue?>

    // With no provider, the core starts on the best quote once every provider answered; the session's state says when.
    suspend fun start(intentId: FundingIntentId, provider: ProductId?): Result<Unit>

    // Not a cancel: one that never started is dropped, one in flight runs on.
    fun leave(intentId: FundingIntentId)

    suspend fun cancel(intentId: FundingIntentId): Result<CancelOutcome>

    // `null` terms keep the session going.
    suspend fun answerCancelOffer(intentId: FundingIntentId, terms: CancelTerms?): Result<CancelConfirmation>

    suspend fun acknowledge(intentId: FundingIntentId)
}

sealed class FundingStartError : Exception() {
    class NoQuote : FundingStartError()

    class Left : FundingStartError()

    class NotSelected(val failure: FundingFailure?) : FundingStartError()
}

// Steps that change the core run on the shared FundingCoreCalls queue, in the order they were made.
@Singleton
class RealFundingSessions @Inject constructor(
    private val core: FundingCore,
    private val quotes: FundingQuotes,
    private val calls: FundingCoreCalls,
    dispatchers: CoroutineDispatchers,
) : FundingSessions {
    private val readDispatcher = dispatchers.io
    private val requests = Channel<FundingEntry>(Channel.UNLIMITED)
    private val changes = MutableSharedFlow<Change>(extraBufferCapacity = CHANGE_BUFFER, onBufferOverflow = BufferOverflow.DROP_OLDEST)

    private val lock = Any()
    private val presentations = mutableMapOf<FundingIntentId, Presentation>()

    override val presentationRequests: Flow<FundingEntry> = requests.receiveAsFlow()

    override fun entries(): Flow<List<FundingEntry>> = changes
        .onSubscription { emit(Change.Entries) }
        .filter { it == Change.Entries }
        .conflate()
        .mapNotNull { readEntries() }
        .flowOn(readDispatcher)

    override fun entryState(intentId: FundingIntentId): Flow<FundingEntry> =
        merge(changes.onSubscription { emit(Change.Entry(intentId)) }, quotes.updates.map(Change::Entry))
            .filter { it == Change.Entry(intentId) }
            .conflate()
            .mapNotNull { readEntry(intentId) }
            .transformWhile { entry ->
                if (!entry.isGone) emit(entry)
                !entry.isGone && !entry.hasEnded
            }
            .flowOn(readDispatcher)

    override fun shown(intentId: FundingIntentId) {
        synchronized(lock) { presentations[intentId]?.shown?.complete(Unit) }
    }

    override suspend fun isAvailable(): Result<Boolean> = core.available().logFailure("Funding providers read failed")

    override suspend fun resume(): Result<Unit> = core.start()

    override suspend fun open(direction: FundingDirection, amount: Balance?): Result<FundingIntentId?> = core.open(direction, amount)

    override fun select(selection: FundingSelection, intentId: FundingIntentId) {
        calls.launch {
            core.select(intentId, selection).logFailure("Funding selection failed")
            changed(intentId)
        }
    }

    override suspend fun amountIssue(amount: Balance, spendable: Balance?, intentId: FundingIntentId): Result<FundingAmountIssue?> =
        core.amountIssue(intentId, amount, spendable).logFailure("Funding amount check failed")

    override suspend fun start(intentId: FundingIntentId, provider: ProductId?): Result<Unit> {
        if (provider == null) return quotes.startOnBest(intentId)

        val quoteId = quotes.quoteId(intentId, provider) ?: return Result.failure(FundingStartError.NoQuote())

        // The session's change can report the start before selectProvider returns, so `selected` wins over presenting.
        return core.selectProvider(intentId, provider, quoteId).flatMap { selected ->
            when {
                selected -> Result.success(resolve(intentId, FundingPresentOutcome.STARTED))
                !isPresenting(intentId) -> Result.failure(FundingStartError.Left())
                else -> {
                    changed(intentId)
                    core.session(intentId).flatMap { Result.failure<Unit>(FundingStartError.NotSelected(it?.stage?.failure)) }
                }
            }
        }
    }

    override fun leave(intentId: FundingIntentId) {
        // The core forgets a start on the best quote itself once the screens are answered dismissed.
        resolve(intentId, FundingPresentOutcome.DISMISSED)
    }

    override suspend fun cancel(intentId: FundingIntentId): Result<CancelOutcome> =
        core.cancel(intentId).also { changed(intentId) }

    override suspend fun answerCancelOffer(intentId: FundingIntentId, terms: CancelTerms?): Result<CancelConfirmation> =
        core.confirmCancel(intentId, terms).also { changed(intentId) }

    override suspend fun acknowledge(intentId: FundingIntentId) {
        core.acknowledge(intentId).logFailure("Funding acknowledgement failed")
        changes.tryEmit(Change.Entries)
    }

    // Screens not shown within SHOWN_TIMEOUT_MS count as dismissed, so a navigation that failed silently does not hold the core.
    suspend fun present(intentId: FundingIntentId, direction: FundingDirection, amount: Balance?): FundingPresentOutcome {
        val presentation = Presentation(direction, amount)
        synchronized(lock) { presentations[intentId] = presentation }
        try {
            readEntry(intentId)?.let { requests.trySend(it) }
            if (withTimeoutOrNull(SHOWN_TIMEOUT_MS) { presentation.shown.await() } == null) {
                Timber.w("Funding %s: its screens did not appear", intentId.value)
                resolve(intentId, FundingPresentOutcome.DISMISSED)
            }
            return presentation.outcome.await()
        } catch (cancellation: CancellationException) {
            resolve(intentId, FundingPresentOutcome.DISMISSED)
            throw cancellation
        }
    }

    // Called on the core's thread, so the read happens elsewhere.
    fun onSessionChanged(intentId: FundingIntentId) {
        calls.launch {
            resolveIfStarted(intentId)
            changed(intentId)
            changes.tryEmit(Change.Entries)
        }
    }

    fun onOptionsChanged(intentId: FundingIntentId) {
        calls.launch { changed(intentId) }
    }

    private fun changed(intentId: FundingIntentId) {
        changes.tryEmit(Change.Entry(intentId))
    }

    private fun isPresenting(intentId: FundingIntentId): Boolean = synchronized(lock) { intentId in presentations }

    private fun resolve(intentId: FundingIntentId, outcome: FundingPresentOutcome) {
        val presentation = synchronized(lock) { presentations.remove(intentId) } ?: return
        quotes.forget(intentId)
        presentation.outcome.complete(outcome)
        changed(intentId)
    }

    // Runs on the call queue. Covers a start on the best quote, which the core makes without a call from here.
    private suspend fun resolveIfStarted(intentId: FundingIntentId) {
        if (!isPresenting(intentId)) return

        core.session(intentId).logFailure("Funding session read failed").onSuccess { session ->
            if (session?.providerId != null) resolve(intentId, FundingPresentOutcome.STARTED)
        }
    }

    // `null` when a read failed; the next change reads again.
    private suspend fun readEntry(intentId: FundingIntentId): FundingEntry? {
        val read = core.session(intentId).flatMap { session ->
            val summary = if (session?.stage?.isOpen == true) Result.success(null) else core.summary(intentId)
            summary.flatMap { ended ->
                val options = if (session?.providerId == null && ended == null) core.options(intentId) else Result.success(null)
                val progress = if (session != null) core.progress(intentId) else Result.success(null)
                options.flatMap { offered -> progress.map { Read(session, ended, offered, it) } }
            }
        }
        val parts = read.logFailure("Funding session read failed").getOrElse { return null }
        val shownQuotes = quotes.latestAsk(intentId)

        return synchronized(lock) {
            val presentation = presentations[intentId]
            FundingEntry.of(
                id = intentId,
                fallbackDirection = presentation?.direction ?: FundingDirection.IN,
                fallbackAmount = presentation?.amount,
                session = parts.session,
                progress = parts.progress,
                summary = parts.summary,
                options = parts.options,
                askedAmount = shownQuotes?.amount,
                quotes = shownQuotes?.list,
                quotesFailed = shownQuotes?.failed == true,
                isPresented = presentation != null,
            )
        }
    }

    private suspend fun readEntries(): List<FundingEntry>? {
        val book = core.entries().logFailure("Funding entries read failed").getOrElse { return null }
        val history = core.history().logFailure("Funding history read failed").getOrElse { return null }
        val held = book.mapTo(mutableSetOf()) { it.session.intent }
        val ended = history.associateBy { it.intent }

        val inBook = book.map { item ->
            val session = item.session
            val intentId = FundingIntentId(session.intent)
            FundingEntry.of(
                id = intentId,
                fallbackDirection = session.direction,
                fallbackAmount = session.amount?.toBalanceOrNull(),
                session = session,
                progress = item.progress,
                // As the history has it, which leaves out a cancel that moved no money.
                summary = ended[session.intent],
                options = null,
                askedAmount = null,
                quotes = null,
                quotesFailed = false,
                isPresented = isPresenting(intentId),
            )
        }
        val past = history.filter { it.intent !in held }.map { summary ->
            FundingEntry.of(
                id = FundingIntentId(summary.intent),
                fallbackDirection = summary.direction,
                fallbackAmount = summary.amount?.toBalanceOrNull(),
                session = null,
                progress = null,
                summary = summary,
                options = null,
                askedAmount = null,
                quotes = null,
                quotesFailed = false,
                isPresented = false,
            )
        }
        return (inBook + past).sortedByDescending { it.openedAtMs }
    }

    private data class Read(
        val session: FundingSession?,
        val summary: FundingSessionSummary?,
        val options: FundingOptions?,
        val progress: FundingProgress?,
    )

    private sealed interface Change {
        data object Entries : Change

        data class Entry(val intentId: FundingIntentId) : Change
    }

    private class Presentation(val direction: FundingDirection, val amount: Balance?) {
        val outcome = CompletableDeferred<FundingPresentOutcome>()

        val shown = CompletableDeferred<Unit>()
    }

    private companion object {
        const val SHOWN_TIMEOUT_MS = 10_000L
        const val CHANGE_BUFFER = 256
    }
}
