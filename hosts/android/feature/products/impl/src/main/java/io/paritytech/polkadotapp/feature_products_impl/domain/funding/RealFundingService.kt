package io.paritytech.polkadotapp.feature_products_impl.domain.funding

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.toBalanceOrNull
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker.WorkerCrashListener
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.channels.BufferOverflow
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.conflate
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.flowOn
import kotlinx.coroutines.flow.mapNotNull
import kotlinx.coroutines.flow.onSubscription
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.flow.transformWhile
import kotlinx.coroutines.withTimeoutOrNull
import timber.log.Timber
import uniffi.truapi.CancelConfirmation
import uniffi.truapi.CancelOutcome
import uniffi.truapi.CancelTerms
import uniffi.truapi.FundingAmountIssue
import uniffi.truapi.FundingChangeKind
import uniffi.truapi.FundingDirection
import uniffi.truapi.FundingFrameOutcome
import uniffi.truapi.FundingFrameTarget
import uniffi.truapi.FundingPresentOutcome
import uniffi.truapi.FundingQuoteList
import uniffi.truapi.FundingSelection
import uniffi.truapi.ProductId
import java.util.UUID
import javax.inject.Inject
import javax.inject.Singleton

/**
 * The one place the funding screens reach the core. Reads what the core holds when it says something changed, and
 * pushes it to whoever follows a session or the list of sessions.
 *
 * Calls that change the core, and the read-backs after its change notices, run one after another in the order they
 * were made, so a quote list is only ever matched against the ask the core last answered.
 */
@Singleton
class RealFundingService @Inject constructor(
    private val core: FundingCore,
    dispatchers: CoroutineDispatchers,
) : FundingService, WorkerCrashListener {
    private val scope = CoroutineScope(SupervisorJob() + dispatchers.io)
    private val readDispatcher = dispatchers.io
    private val calls = FundingCoreCalls(scope)
    private val eventQueue = Channel<FundingEvent>(Channel.UNLIMITED)
    private val changes = MutableSharedFlow<Change>(extraBufferCapacity = CHANGE_BUFFER, onBufferOverflow = BufferOverflow.DROP_OLDEST)

    private val lock = Any()
    private val presentations = mutableMapOf<FundingSessionId, Presentation>()
    private val frames = mutableMapOf<FundingFrameRequestId, PendingFrame>()
    private val asks = mutableMapOf<FundingSessionId, QuoteAsk>()
    private val providers = mutableSetOf<ProductId>()

    override val events: Flow<FundingEvent> = eventQueue.receiveAsFlow()

    override fun entries(): Flow<List<FundingEntry>> = changes
        .onSubscription { emit(Change.Entries) }
        .filter { it == Change.Entries }
        .conflate()
        .mapNotNull { readEntries() }
        .flowOn(readDispatcher)

    override fun entryState(id: FundingSessionId): Flow<FundingEntry> = changes
        .onSubscription { emit(Change.Entry(id)) }
        .filter { it == Change.Entry(id) }
        .conflate()
        .mapNotNull { readEntry(id) }
        .transformWhile { entry ->
            if (!entry.isGone) emit(entry)
            !entry.isGone && !entry.hasEnded
        }
        .flowOn(readDispatcher)

    override fun shown(id: FundingSessionId) {
        synchronized(lock) { presentations[id]?.shown?.complete(Unit) }
    }

    override suspend fun isAvailable(): Result<Boolean> = core.available().logFailure("Funding providers read failed")

    override suspend fun resume(): Result<Unit> = core.start()

    override suspend fun open(direction: FundingDirection, amount: Balance?): Result<FundingSessionId?> = core.open(direction, amount).map { it?.let(::FundingSessionId) }

    override fun select(selection: FundingSelection, id: FundingSessionId) {
        calls.launch {
            core.select(id.value, selection).logFailure("Funding selection failed")
            changed(id)
        }
    }

    override fun quote(amount: Balance?, id: FundingSessionId) {
        if (amount == null) {
            synchronized(lock) { asks[id]?.drop() }
            changed(id)
            return
        }

        val number = synchronized(lock) { asks.getOrPut(id) { QuoteAsk() }.ask(amount) }
        changed(id)
        calls.launch {
            val requestId = core.requestQuote(id.value, amount).logFailure("Funding quote request failed").getOrNull()
            val startsNow = synchronized(lock) { asks[id]?.take(requestId, number) ?: false }
            takeQuotes(id)
            if (startsNow) startWhenQuoted(id)
        }
    }

    override suspend fun amountIssue(amount: Balance, spendable: Balance?, id: FundingSessionId): Result<FundingAmountIssue?> =
        core.amountIssue(id.value, amount, spendable).logFailure("Funding amount check failed")

    override suspend fun start(id: FundingSessionId, provider: ProductId?): Result<Unit> {
        if (provider == null) {
            synchronized(lock) { asks.getOrPut(id) { QuoteAsk() }.startsWhenQuoted = true }
            calls.run { startWhenQuoted(id) }
            return Result.success(Unit)
        }

        val quoteId = synchronized(lock) {
            asks[id]?.shown?.rows?.firstOrNull { it.providerId == provider }?.state?.quote?.quoteId
        } ?: return Result.failure(FundingStartError.NoQuote())

        return core.selectProvider(id.value, provider, quoteId).flatMap { selected ->
            when {
                !isPresenting(id) -> Result.failure(FundingStartError.Left())
                !selected -> {
                    changed(id)
                    Result.failure(FundingStartError.NotSelected(core.session(id.value).getOrNull()?.stage?.failure))
                }
                else -> Result.success(resolve(id, FundingPresentOutcome.STARTED))
            }
        }
    }

    override fun stopStarting(id: FundingSessionId) {
        val wasStarting = synchronized(lock) { asks[id]?.stopStarting() ?: false }
        if (wasStarting) calls.launch { core.cancelAutoStart(id.value).logFailure("Funding auto-start cancel failed") }
    }

    override fun leave(id: FundingSessionId) {
        if (!isPresenting(id)) return

        stopStarting(id)
        resolve(id, FundingPresentOutcome.DISMISSED)
    }

    override suspend fun cancel(id: FundingSessionId): Result<CancelOutcome> {
        if (isPresenting(id)) {
            leave(id)
            return Result.success(CancelOutcome.ENDED)
        }
        return core.cancel(id.value).also { changed(id) }
    }

    override suspend fun answerCancelOffer(id: FundingSessionId, terms: CancelTerms?): Result<CancelConfirmation> =
        core.confirmCancel(id.value, terms).also { changed(id) }

    override suspend fun acknowledge(id: FundingSessionId) {
        core.acknowledge(id.value).logFailure("Funding acknowledgement failed")
        changes.tryEmit(Change.Entries)
    }

    override suspend fun openProviderFrame(sessionId: FundingSessionId): Result<Boolean> =
        core.openFrame(sessionId.value).logFailure("Funding provider screen could not open")

    override suspend fun isFrameWaiting(sessionId: FundingSessionId): Result<Boolean> =
        core.frameWaiting(sessionId.value).logFailure("Funding provider screen read failed")

    override fun answerFrame(request: FundingFrameRequest, outcome: FundingFrameOutcome) {
        val pending = synchronized(lock) { frames.remove(request.id) }
        pending?.answer?.complete(outcome)
    }

    /**
     * The core waits on the session's screens. A session whose screens are not [shown] within [SHOWN_TIMEOUT_MS] is
     * taken as dismissed, so a navigation that silently failed does not hold the core forever.
     */
    suspend fun present(id: FundingSessionId, direction: FundingDirection, amount: Balance?): FundingPresentOutcome {
        val presentation = Presentation(direction, amount)
        synchronized(lock) { presentations[id] = presentation }
        try {
            readEntry(id)?.let { eventQueue.trySend(FundingEvent.PresentationRequested(it)) }
            if (withTimeoutOrNull(SHOWN_TIMEOUT_MS) { presentation.shown.await() } == null) {
                Timber.w("Funding %s: its screens did not appear", id)
                resolve(id, FundingPresentOutcome.DISMISSED)
            }
            return presentation.outcome.await()
        } catch (cancellation: CancellationException) {
            resolve(id, FundingPresentOutcome.DISMISSED)
            throw cancellation
        }
    }

    /** Provider [providerId] asks to show a screen for session [id], which the core holds until the app opens it. */
    fun frameRequested(providerId: ProductId, id: FundingSessionId) {
        eventQueue.trySend(FundingEvent.FrameRequested(id, providerId))
    }

    /** A provider's screen the core presents now, once the app opened it, answered once the app takes it down. */
    suspend fun presentFrame(providerId: ProductId, id: FundingSessionId, target: FundingFrameTarget): FundingFrameOutcome {
        val request = FundingFrameRequest(
            id = FundingFrameRequestId(UUID.randomUUID().toString()),
            sessionId = id,
            providerId = providerId,
            target = target,
        )
        val frame = PendingFrame(request)
        synchronized(lock) { frames[request.id] = frame }
        try {
            eventQueue.trySend(FundingEvent.FrameOpened(request))
            return frame.answer.await()
        } catch (cancellation: CancellationException) {
            synchronized(lock) { frames.remove(request.id) }
            eventQueue.trySend(FundingEvent.FrameClosed(id))
            throw cancellation
        }
    }

    /** The provider is done with its screens for session [id]: each one still open answers `Closed`. */
    fun closeFrames(providerId: ProductId, id: FundingSessionId) {
        val closed = synchronized(lock) {
            frames.values
                .filter { it.request.sessionId == id && it.request.providerId == providerId }
                .onEach { frames.remove(it.request.id) }
        }
        closed.forEach { it.answer.complete(FundingFrameOutcome.Closed) }
        eventQueue.trySend(FundingEvent.FrameClosed(id))
    }

    /** The core says [kind] changed for session [id]. Called on the core's thread, so the read happens elsewhere. */
    fun changed(id: FundingSessionId?, kind: FundingChangeKind) {
        if (id == null) return
        calls.launch {
            when (kind) {
                FundingChangeKind.SESSION -> {
                    startedWhenQuoted(id)
                    changed(id)
                    changes.tryEmit(Change.Entries)
                }
                FundingChangeKind.OPTIONS -> changed(id)
                FundingChangeKind.QUOTES -> takeQuotes(id)
                FundingChangeKind.PROVIDERS -> Unit
            }
        }
    }

    /** A provider of a session this service has seen is reported through [events] instead of the generic notice. */
    override fun onWorkerCrashing(productId: ProductId): Boolean {
        val provides = synchronized(lock) { productId in providers }
        if (provides) eventQueue.trySend(FundingEvent.ProviderNotWorking(productId))
        return provides
    }

    private fun changed(id: FundingSessionId) {
        changes.tryEmit(Change.Entry(id))
    }

    private fun isPresenting(id: FundingSessionId): Boolean = synchronized(lock) { id in presentations }

    private fun resolve(id: FundingSessionId, outcome: FundingPresentOutcome) {
        val presentation = synchronized(lock) { presentations.remove(id) }
        presentation?.outcome?.complete(outcome)
        changed(id)
    }

    /** Asks the core to start the session on the latest ask's best quote. Runs on the call queue. */
    private suspend fun startWhenQuoted(id: FundingSessionId) {
        val requestId = synchronized(lock) { asks[id]?.armedRequest() } ?: return
        core.startWhenQuoted(id.value, requestId).logFailure("Funding auto-start failed")
    }

    /** A session the core handed to the best quote it was asked to start on is reported started. */
    private suspend fun startedWhenQuoted(id: FundingSessionId) {
        val armed = synchronized(lock) { asks[id]?.startsWhenQuoted == true } && isPresenting(id)
        if (!armed || core.session(id.value).getOrNull()?.providerId == null) return

        synchronized(lock) { asks[id]?.startsWhenQuoted = false }
        resolve(id, FundingPresentOutcome.STARTED)
    }

    private suspend fun takeQuotes(id: FundingSessionId) {
        val list = core.quotes(id.value).logFailure("Funding quotes read failed").getOrNull()
        synchronized(lock) { asks[id]?.receive(list) }
        changed(id)
    }

    private suspend fun readEntry(id: FundingSessionId): FundingEntry? {
        val session = core.session(id.value).logFailure("Funding session read failed").getOrElse { return null }
        val summary = if (session?.stage?.isOpen == true) null else core.summary(id.value).logFailure("Funding summary read failed").getOrNull()
        val options = if (session?.providerId == null && summary == null) core.options(id.value).logFailure("Funding options read failed").getOrNull() else null
        val progress = session?.let { core.progress(id.value).logFailure("Funding progress read failed").getOrNull() }

        return synchronized(lock) {
            remember(options?.candidates?.map { it.providerId })
            remember(listOfNotNull(session?.providerId))
            val presentation = presentations[id]
            val ask = asks[id]
            FundingEntry.of(
                id = id,
                fallbackDirection = presentation?.direction ?: FundingDirection.IN,
                fallbackAmount = presentation?.amount,
                session = session,
                progress = progress,
                summary = summary,
                options = options,
                askedAmount = ask?.amount,
                quotes = ask?.shown,
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
        synchronized(lock) { remember(book.mapNotNull { it.session.providerId }) }

        val inBook = book.map { item ->
            val session = item.session
            FundingEntry.of(
                id = FundingSessionId(session.intent),
                fallbackDirection = session.direction,
                fallbackAmount = session.amount?.toBalanceOrNull(),
                session = session,
                progress = item.progress,
                // As the history has it, which leaves out a cancel that moved no money.
                summary = ended[session.intent],
                options = null,
                askedAmount = null,
                quotes = null,
                isPresented = isPresenting(FundingSessionId(session.intent)),
            )
        }
        val past = history.filter { it.intent !in held }.map { summary ->
            FundingEntry.of(
                id = FundingSessionId(summary.intent),
                fallbackDirection = summary.direction,
                fallbackAmount = summary.amount?.toBalanceOrNull(),
                session = null,
                progress = null,
                summary = summary,
                options = null,
                askedAmount = null,
                quotes = null,
                isPresented = false,
            )
        }
        return (inBook + past).sortedByDescending { it.openedAtMs }
    }

    private fun remember(providerIds: List<ProductId>?) {
        providerIds?.let(providers::addAll)
    }

    private sealed interface Change {
        data object Entries : Change

        data class Entry(val id: FundingSessionId) : Change
    }

    private class Presentation(val direction: FundingDirection, val amount: Balance?) {
        val outcome = CompletableDeferred<FundingPresentOutcome>()

        val shown = CompletableDeferred<Unit>()
    }

    private class PendingFrame(val request: FundingFrameRequest) {
        val answer = CompletableDeferred<FundingFrameOutcome>()
    }

    /** The quotes of one session: only the latest ask's list is shown. Guarded by the service's lock. */
    private class QuoteAsk {
        private var number = 0

        /** The core's id for the latest ask, once it returned it. */
        private var requestId: ULong? = null

        /** The ask the core was last told to start the session on. */
        private var startedOn: ULong? = null

        private var awaiting = false

        var amount: Balance? = null
            private set
        var shown: FundingQuoteList? = null
            private set

        /** The session is handed to the latest ask's best quote once every provider answered. */
        var startsWhenQuoted = false

        fun ask(asked: Balance): Int {
            amount = asked
            shown = null
            awaiting = true
            requestId = null
            return ++number
        }

        fun drop() {
            number++
            amount = null
            shown = null
            awaiting = false
            requestId = null
        }

        /** Takes the core's id for ask [ofAsk] unless a later ask replaced it; answers whether to start on it now. */
        fun take(id: ULong?, ofAsk: Int): Boolean {
            if (ofAsk != number) return false
            if (id == null) {
                drop()
                return false
            }
            requestId = id
            return startsWhenQuoted
        }

        /**
         * The core's latest list: shown when it answers the latest ask. None while an ask waits for its first list is
         * the list not being there yet; none after it is a selection having withdrawn it.
         */
        fun receive(list: FundingQuoteList?) {
            if (list == null) {
                if (!awaiting) {
                    amount = null
                    shown = null
                }
                return
            }
            if (list.requestId != requestId) return

            shown = list
            awaiting = false
        }

        /** The request to start on, once per request. */
        fun armedRequest(): ULong? {
            val request = requestId?.takeIf { startsWhenQuoted && it != startedOn } ?: return null
            startedOn = request
            return request
        }

        fun stopStarting(): Boolean {
            val was = startsWhenQuoted
            startsWhenQuoted = false
            startedOn = null
            return was
        }
    }

    private companion object {
        const val SHOWN_TIMEOUT_MS = 10_000L
        const val CHANGE_BUFFER = 256
    }
}
