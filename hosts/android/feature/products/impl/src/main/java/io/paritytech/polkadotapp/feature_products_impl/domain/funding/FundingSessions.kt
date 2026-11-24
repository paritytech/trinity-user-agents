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

/**
 * The sessions the core holds: the list, one session's state, and the user's steps on it. The core owns every
 * session: the screens follow its state, report what the user picked, and leave every decision to the core.
 */
interface FundingSessions {
    /** Every session the core holds, in flight and ended, newest first: the current list, then a new one on every change. Never ends. */
    fun entries(): Flow<List<FundingEntry>>

    /**
     * Session [intentId] as it changes, current state first, with the options and quotes for it. Completes after the
     * session ended, and without a value once it is gone: dropped by the core or left before it started.
     */
    fun entryState(intentId: FundingIntentId): Flow<FundingEntry>

    /** The new sessions whose screens the core asks for. One consumer reads it; each waits until it does. Never ends. */
    val presentationRequests: Flow<FundingEntry>

    /** The screens of session [intentId] are up, so the core no longer takes it as dismissed when they do not appear. */
    fun shown(intentId: FundingIntentId)

    /** Starts the core, which resumes the sessions it holds. */
    suspend fun resume(): Result<Unit>

    /**
     * Opens a session on the host's own behalf, for [amount] in the balance asset's planks or what the user types when
     * `null`. Its screens are asked for through [presentationRequests]; answers its id, or `null` once the user left it.
     */
    suspend fun open(direction: FundingDirection, amount: Balance?): Result<FundingIntentId?>

    /** Tells the core what the user picked, in order with the asks after it. What it leads to comes back as options. */
    fun select(selection: FundingSelection, intentId: FundingIntentId)

    /** Why [amount] cannot go ahead for the reported selection, given what a withdrawal can spend. */
    suspend fun amountIssue(amount: Balance, spendable: Balance?, intentId: FundingIntentId): Result<FundingAmountIssue?>

    /**
     * Hands the session to [provider] on the quote on screen. With no provider the core hands it to the latest ask's
     * best quote once every provider answered, and the session's state says when it did. Fails with a
     * [FundingStartError] when the provider did not take it.
     */
    suspend fun start(intentId: FundingIntentId, provider: ProductId?): Result<Unit>

    /** The user left the session's screens. One that never started is dropped; one in flight runs on. */
    fun leave(intentId: FundingIntentId)

    /**
     * Cancels a session. One that never started is dropped at once, as when the user leaves it; one in flight goes to
     * the core, which ends it or asks the provider.
     */
    suspend fun cancel(intentId: FundingIntentId): Result<CancelOutcome>

    /** Answers the provider's cancel offer: [terms] are the ones the user accepted, as shown, or `null` to keep going. */
    suspend fun answerCancelOffer(intentId: FundingIntentId, terms: CancelTerms?): Result<CancelConfirmation>

    /** The user was shown how session [intentId] ended. */
    suspend fun acknowledge(intentId: FundingIntentId)
}

/** Why the session could not be handed to the chosen provider. */
sealed class FundingStartError : Exception() {
    /** No quote from that provider is on screen. */
    class NoQuote : FundingStartError()

    /** The user left before the provider took the session. */
    class Left : FundingStartError()

    /** The provider would not take it; [failure] says why when the core recorded one. */
    class NotSelected(val failure: FundingFailure?) : FundingStartError()
}

/**
 * Reads what the core holds when it says a session changed, and pushes it to whoever follows a session or the list.
 * Steps that change the core run on the shared [FundingCoreCalls] queue, in the order they were made.
 */
@Singleton
class RealFundingSessions @Inject constructor(
    private val core: FundingCore,
    private val quotes: RealFundingQuotes,
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
        if (provider == null) {
            quotes.startOnBest(intentId)
            return Result.success(Unit)
        }

        val quoteId = quotes.quoteId(intentId, provider) ?: return Result.failure(FundingStartError.NoQuote())

        return core.selectProvider(intentId, provider, quoteId).flatMap { selected ->
            when {
                !isPresenting(intentId) -> Result.failure(FundingStartError.Left())
                !selected -> {
                    changed(intentId)
                    core.session(intentId).flatMap { Result.failure<Unit>(FundingStartError.NotSelected(it?.stage?.failure)) }
                }
                else -> Result.success(resolve(intentId, FundingPresentOutcome.STARTED))
            }
        }
    }

    override fun leave(intentId: FundingIntentId) {
        if (!isPresenting(intentId)) return

        quotes.stopStarting(intentId)
        resolve(intentId, FundingPresentOutcome.DISMISSED)
    }

    override suspend fun cancel(intentId: FundingIntentId): Result<CancelOutcome> {
        if (isPresenting(intentId)) {
            leave(intentId)
            return Result.success(CancelOutcome.ENDED)
        }
        return core.cancel(intentId).also { changed(intentId) }
    }

    override suspend fun answerCancelOffer(intentId: FundingIntentId, terms: CancelTerms?): Result<CancelConfirmation> =
        core.confirmCancel(intentId, terms).also { changed(intentId) }

    override suspend fun acknowledge(intentId: FundingIntentId) {
        core.acknowledge(intentId).logFailure("Funding acknowledgement failed")
        changes.tryEmit(Change.Entries)
    }

    /**
     * The core waits on the session's screens. A session whose screens are not [shown] within [SHOWN_TIMEOUT_MS] is
     * taken as dismissed, so a navigation that silently failed does not hold the core forever.
     */
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

    /** The core says session [intentId] changed. Called on the core's thread, so the read happens elsewhere. */
    fun onSessionChanged(intentId: FundingIntentId) {
        calls.launch {
            startedOnBest(intentId)
            changed(intentId)
            changes.tryEmit(Change.Entries)
        }
    }

    /** The core says what it offers for session [intentId] changed. */
    fun onOptionsChanged(intentId: FundingIntentId) {
        calls.launch { changed(intentId) }
    }

    private fun changed(intentId: FundingIntentId) {
        changes.tryEmit(Change.Entry(intentId))
    }

    private fun isPresenting(intentId: FundingIntentId): Boolean = synchronized(lock) { intentId in presentations }

    private fun resolve(intentId: FundingIntentId, outcome: FundingPresentOutcome) {
        val presentation = synchronized(lock) { presentations.remove(intentId) }
        presentation?.outcome?.complete(outcome)
        changed(intentId)
    }

    /** A session the core handed to the best quote it was asked to start on is reported started. Runs on the call queue. */
    private suspend fun startedOnBest(intentId: FundingIntentId) {
        if (!quotes.isStartingOnBest(intentId) || !isPresenting(intentId)) return

        core.session(intentId).logFailure("Funding session read failed").onSuccess { session ->
            if (session?.providerId == null) return@onSuccess
            quotes.startedOnBest(intentId)
            resolve(intentId, FundingPresentOutcome.STARTED)
        }
    }

    /** Session [intentId] as the core holds it; `null` when a read failed, which the next change reads again. */
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
        val shownQuotes = quotes.shown(intentId)

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

    /** Every session the core holds, newest first: those it holds in full, and the ended ones from its history. */
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
