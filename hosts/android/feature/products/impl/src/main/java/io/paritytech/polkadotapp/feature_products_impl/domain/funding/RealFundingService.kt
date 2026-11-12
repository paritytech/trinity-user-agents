package io.paritytech.polkadotapp.feature_products_impl.domain.funding

import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.logFailure
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
import uniffi.truapi.FundingQuote
import uniffi.truapi.FundingQuoteList
import uniffi.truapi.FundingSelection
import uniffi.truapi.ProductId
import uniffi.truapi.U128
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
) : FundingService {
    private val scope = CoroutineScope(SupervisorJob() + dispatchers.io)
    private val readDispatcher = dispatchers.io
    private val calls = FundingCoreCalls(scope)
    private val eventQueue = Channel<FundingEvent>(Channel.UNLIMITED)
    private val changes = MutableSharedFlow<Change>(extraBufferCapacity = CHANGE_BUFFER, onBufferOverflow = BufferOverflow.DROP_OLDEST)

    private val lock = Any()
    private val presentations = mutableMapOf<String, Presentation>()
    private val frames = mutableMapOf<String, PendingFrame>()
    private val asks = mutableMapOf<String, QuoteAsk>()
    private val providers = mutableSetOf<ProductId>()

    override val events: Flow<FundingEvent> = eventQueue.receiveAsFlow()

    override fun entries(): Flow<List<FundingEntry>> = changes
        .onSubscription { emit(Change.Entries) }
        .filter { it == Change.Entries }
        .conflate()
        .mapNotNull { readEntries() }
        .flowOn(readDispatcher)

    override fun entryState(id: String): Flow<FundingEntry> = changes
        .onSubscription {
            synchronized(lock) { presentations[id]?.shown?.complete(Unit) }
            emit(Change.Entry(id))
        }
        .filter { it == Change.Entry(id) }
        .conflate()
        .mapNotNull { readEntry(id) }
        .transformWhile { entry ->
            if (!entry.isGone) emit(entry)
            !entry.isGone && !entry.hasEnded
        }
        .flowOn(readDispatcher)

    override suspend fun resume(): Result<Unit> = core.start()

    override suspend fun open(direction: FundingDirection, amount: U128?): Result<String?> = core.open(direction, amount)

    override fun select(selection: FundingSelection, id: String) {
        calls.launch {
            core.select(id, selection).logFailure("Funding selection failed")
            changed(id)
        }
    }

    override fun quote(amount: U128?, id: String) {
        if (amount == null) {
            synchronized(lock) { asks[id]?.drop() }
            changed(id)
            return
        }

        val number = synchronized(lock) { asks.getOrPut(id) { QuoteAsk() }.ask(amount) }
        changed(id)
        calls.launch {
            val requestId = core.requestQuote(id, amount).logFailure("Funding quote request failed").getOrNull()
            val startsNow = synchronized(lock) { asks[id]?.take(requestId, number) ?: false }
            takeQuotes(id)
            if (startsNow) startWhenQuoted(id)
        }
    }

    override suspend fun amountIssue(amount: U128, spendable: U128?, id: String): FundingAmountIssue? =
        core.amountIssue(id, amount, spendable).logFailure("Funding amount check failed").getOrNull()

    override suspend fun start(id: String, provider: ProductId?): Result<Unit> {
        if (provider == null) {
            synchronized(lock) { asks.getOrPut(id) { QuoteAsk() }.startsWhenQuoted = true }
            calls.run { startWhenQuoted(id) }
            return Result.success(Unit)
        }

        val quoteId = synchronized(lock) {
            asks[id]?.shown?.rows?.firstOrNull { it.providerId == provider }?.state?.quote?.quoteId
        } ?: return Result.failure(FundingStartError.NoQuote())

        return core.selectProvider(id, provider, quoteId).mapCatching { selected ->
            when {
                !isPresenting(id) -> throw FundingStartError.Left()
                !selected -> {
                    changed(id)
                    throw FundingStartError.NotSelected(core.session(id).getOrNull()?.stage?.failure)
                }
                else -> resolve(id, FundingPresentOutcome.STARTED)
            }
        }
    }

    override fun stopStarting(id: String) {
        val wasStarting = synchronized(lock) { asks[id]?.stopStarting() ?: false }
        if (wasStarting) calls.launch { core.cancelAutoStart(id).logFailure("Funding auto-start cancel failed") }
    }

    override fun leave(id: String) {
        if (!isPresenting(id)) return

        stopStarting(id)
        resolve(id, FundingPresentOutcome.DISMISSED)
    }

    override suspend fun cancel(id: String): Result<CancelOutcome> {
        if (isPresenting(id)) {
            leave(id)
            return Result.success(CancelOutcome.ENDED)
        }
        return core.cancel(id).also { changed(id) }
    }

    override suspend fun answerCancelOffer(id: String, terms: CancelTerms?): Result<CancelConfirmation> =
        core.confirmCancel(id, terms).also { changed(id) }

    override suspend fun acknowledge(id: String) {
        core.acknowledge(id).logFailure("Funding acknowledgement failed")
        changes.tryEmit(Change.Entries)
    }

    override fun openProviderFrame(request: FundingFrameRequest) {
        // TODO(#2057): call the core's `open_provider_frame(request.entryId)` here. Until then the core presents a
        //  provider's screen as soon as the provider asks, so the request is already the screen and nothing is held.
        Timber.d("Funding %s: opening %s's screen", request.entryId, request.providerId)
    }

    override fun answerFrame(request: FundingFrameRequest, outcome: FundingFrameOutcome) {
        val pending = synchronized(lock) { frames.remove(request.id) }
        pending?.answer?.complete(outcome)
    }

    /**
     * The core waits on the session's screens. A session nobody follows within [SHOWN_TIMEOUT_MS] of asking for its
     * screens is taken as dismissed, so a navigation that silently failed does not hold the core forever.
     */
    suspend fun present(intent: String, direction: FundingDirection, amount: U128?): FundingPresentOutcome {
        val presentation = Presentation(direction, amount)
        synchronized(lock) { presentations[intent] = presentation }
        try {
            readEntry(intent)?.let { eventQueue.trySend(FundingEvent.PresentationRequested(it)) }
            if (withTimeoutOrNull(SHOWN_TIMEOUT_MS) { presentation.shown.await() } == null) {
                Timber.w("Funding %s: its screens did not appear", intent)
                resolve(intent, FundingPresentOutcome.DISMISSED)
            }
            return presentation.outcome.await()
        } catch (cancellation: CancellationException) {
            resolve(intent, FundingPresentOutcome.DISMISSED)
            throw cancellation
        }
    }

    /** A provider's screen the core presents now, answered once the app takes it down. */
    suspend fun presentFrame(providerId: ProductId, intent: String, target: FundingFrameTarget): FundingFrameOutcome {
        val request = FundingFrameRequest(id = UUID.randomUUID().toString(), entryId = intent, providerId = providerId, target = target)
        val frame = PendingFrame(request)
        synchronized(lock) { frames[request.id] = frame }
        try {
            eventQueue.trySend(FundingEvent.FrameRequested(request))
            return frame.answer.await()
        } catch (cancellation: CancellationException) {
            synchronized(lock) { frames.remove(request.id) }
            eventQueue.trySend(FundingEvent.FrameClosed(intent))
            throw cancellation
        }
    }

    /** The provider is done with its screens for session [intent]: each one still open answers `Closed`. */
    fun closeFrames(providerId: ProductId, intent: String) {
        val closed = synchronized(lock) {
            frames.values
                .filter { it.request.entryId == intent && it.request.providerId == providerId }
                .onEach { frames.remove(it.request.id) }
        }
        closed.forEach { it.answer.complete(FundingFrameOutcome.Closed) }
        eventQueue.trySend(FundingEvent.FrameClosed(intent))
    }

    /** The core says [kind] changed for session [intent]. Called on the core's thread, so the read happens elsewhere. */
    fun changed(intent: String?, kind: FundingChangeKind) {
        if (intent == null) return
        calls.launch {
            when (kind) {
                FundingChangeKind.SESSION -> {
                    startedWhenQuoted(intent)
                    changed(intent)
                    changes.tryEmit(Change.Entries)
                }
                FundingChangeKind.OPTIONS -> changed(intent)
                FundingChangeKind.QUOTES -> takeQuotes(intent)
                FundingChangeKind.PROVIDERS -> Unit
            }
        }
    }

    /**
     * A product's worker keeps crashing. Answers whether it provides a session this service has seen, which the app
     * is then told of through [events].
     */
    fun workerCrashing(productId: ProductId): Boolean {
        val provides = synchronized(lock) { productId in providers }
        if (provides) eventQueue.trySend(FundingEvent.ProviderNotWorking(productId))
        return provides
    }

    private fun changed(id: String) {
        changes.tryEmit(Change.Entry(id))
    }

    private fun isPresenting(id: String): Boolean = synchronized(lock) { id in presentations }

    private fun resolve(id: String, outcome: FundingPresentOutcome) {
        val presentation = synchronized(lock) { presentations.remove(id) }
        presentation?.outcome?.complete(outcome)
        changed(id)
    }

    /** Asks the core to start the session on the latest ask's best quote. Runs on the call queue. */
    private suspend fun startWhenQuoted(id: String) {
        val requestId = synchronized(lock) { asks[id]?.armedRequest() } ?: return
        core.startWhenQuoted(id, requestId).logFailure("Funding auto-start failed")
    }

    /** A session the core handed to the best quote it was asked to start on is reported started. */
    private suspend fun startedWhenQuoted(id: String) {
        val armed = synchronized(lock) { asks[id]?.startsWhenQuoted == true } && isPresenting(id)
        if (!armed || core.session(id).getOrNull()?.providerId == null) return

        synchronized(lock) { asks[id]?.startsWhenQuoted = false }
        resolve(id, FundingPresentOutcome.STARTED)
    }

    private suspend fun takeQuotes(id: String) {
        val list = core.quotes(id).logFailure("Funding quotes read failed").getOrNull()
        synchronized(lock) { asks[id]?.receive(list) }
        changed(id)
    }

    private suspend fun readEntry(id: String): FundingEntry? {
        val session = core.session(id).logFailure("Funding session read failed").getOrElse { return null }
        val summary = if (session?.stage?.isOpen == true) null else core.summary(id).getOrNull()
        val options = if (session?.providerId == null && summary == null) core.options(id).getOrNull() else null
        val progress = session?.let { core.progress(id).getOrNull() }

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
                quotes = ask?.shown,
                staleQuotes = ask?.stale.orEmpty(),
                isQuoting = ask?.awaiting == true,
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
                id = session.intent,
                fallbackDirection = session.direction,
                fallbackAmount = session.amount,
                session = session,
                progress = item.progress,
                // As the history has it, which leaves out a cancel that moved no money.
                summary = ended[session.intent],
                options = null,
                quotes = null,
                staleQuotes = emptyMap(),
                isQuoting = false,
                isPresented = isPresenting(session.intent),
            )
        }
        val past = history.filter { it.intent !in held }.map { summary ->
            FundingEntry.of(
                id = summary.intent,
                fallbackDirection = summary.direction,
                fallbackAmount = summary.amount,
                session = null,
                progress = null,
                summary = summary,
                options = null,
                quotes = null,
                staleQuotes = emptyMap(),
                isQuoting = false,
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

        data class Entry(val id: String) : Change
    }

    private class Presentation(val direction: FundingDirection, val amount: U128?) {
        val outcome = CompletableDeferred<FundingPresentOutcome>()

        /** Something follows the session, so its screens are up. */
        val shown = CompletableDeferred<Unit>()
    }

    private class PendingFrame(val request: FundingFrameRequest) {
        val answer = CompletableDeferred<FundingFrameOutcome>()
    }

    /** The quotes of one session: only the latest ask's list is shown. Guarded by the service's lock. */
    private class QuoteAsk {
        private var number = 0
        private var amount: U128? = null

        /** The core's id for the latest ask, once it returned it. */
        private var requestId: ULong? = null

        /** The ask the core was last told to start the session on. */
        private var startedOn: ULong? = null

        var shown: FundingQuoteList? = null
            private set
        var stale: Map<ProductId, FundingQuote> = emptyMap()
            private set
        var awaiting = false
            private set

        /** The session is handed to the latest ask's best quote once every provider answered. */
        var startsWhenQuoted = false

        /** A new ask of [asked]: the quotes on screen for the same amount stay, greyed, until they are answered again. */
        fun ask(asked: U128): Int {
            stale = if (asked == amount) shown?.quotes().orEmpty() + stale else emptyMap()
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
            stale = emptyMap()
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
                    shown = null
                    stale = emptyMap()
                }
                return
            }
            if (list.requestId != requestId) return

            val previous = shown?.quotes().orEmpty() + stale
            stale = list.rows.filter { it.state.isPending }.mapNotNull { row -> previous[row.providerId]?.let { row.providerId to it } }.toMap()
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

        private fun FundingQuoteList.quotes(): Map<ProductId, FundingQuote> =
            rows.mapNotNull { row -> row.state.quote?.let { row.providerId to it } }.toMap()
    }

    private companion object {
        const val SHOWN_TIMEOUT_MS = 10_000L
        const val CHANGE_BUFFER = 256
    }
}
