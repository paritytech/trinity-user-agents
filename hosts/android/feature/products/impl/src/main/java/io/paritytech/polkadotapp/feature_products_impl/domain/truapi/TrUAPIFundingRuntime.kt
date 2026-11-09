package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.paritytech.polkadotapp.common.utils.CoroutineDispatchers
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingRuntime
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.channels.BufferOverflow
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asSharedFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import uniffi.truapi.CancelConfirmation
import uniffi.truapi.CancelOutcome
import uniffi.truapi.CancelTerms
import uniffi.truapi.FundingBookEntry
import uniffi.truapi.FundingChangeKind
import uniffi.truapi.FundingDirection
import uniffi.truapi.FundingOptions
import uniffi.truapi.FundingQuoteAsk
import uniffi.truapi.FundingQuoteList
import uniffi.truapi.FundingSelection
import uniffi.truapi.FundingSessionSummary
import uniffi.truapi.ProductId
import javax.inject.Inject
import javax.inject.Singleton

/**
 * The funding screens' view of the core. The core keeps the sessions, options and quote lists; on each change notice
 * this reads the new state back from the runtime, off the core's thread and in the order the notices came, and
 * holds the latest for the screens to observe.
 */
@Singleton
class TrUAPIFundingRuntime @Inject constructor(
    private val runtimeProvider: TrUAPIHostRuntimeProvider,
    dispatchers: CoroutineDispatchers,
) : FundingRuntime {
    private val scope = CoroutineScope(SupervisorJob() + dispatchers.computation)
    private val notices = Channel<FundingChange>(Channel.UNLIMITED)

    private val entries = MutableStateFlow<Map<String, FundingBookEntry>>(emptyMap())
    private val historyState = MutableStateFlow<List<FundingSessionSummary>>(emptyList())
    private val optionsByIntent = MutableStateFlow<Map<String, FundingOptions>>(emptyMap())
    private val changedSessions = MutableSharedFlow<String>(
        extraBufferCapacity = EVENT_BUFFER,
        onBufferOverflow = BufferOverflow.DROP_OLDEST,
    )
    private val quoteLists = MutableSharedFlow<FundingQuoteListUpdate>(
        extraBufferCapacity = EVENT_BUFFER,
        onBufferOverflow = BufferOverflow.DROP_OLDEST,
    )

    init {
        scope.launch {
            for (notice in notices) readBack(notice)
        }
    }

    /** The core says funding state changed. Called on the core's thread, so the read happens elsewhere. */
    fun onChanged(intent: String?, kind: FundingChangeKind) {
        notices.trySend(FundingChange(intent, kind))
    }

    override suspend fun openFunding(direction: FundingDirection): Result<String?> =
        runtimeProvider.runtime().mapCatching { it.openFunding(direction, null) }

    override suspend fun start(): Result<Unit> = runtimeProvider.runtime().mapCatching { historyState.value = it.fundingHistory() }

    override val sessions: StateFlow<Map<String, FundingBookEntry>> = entries.asStateFlow()

    override val history: StateFlow<List<FundingSessionSummary>> = historyState.asStateFlow()

    override suspend fun summary(intent: String): Result<FundingSessionSummary?> =
        runtimeProvider.runtime().mapCatching { it.fundingSummary(intent) }

    override fun options(intent: String): Flow<FundingOptions?> =
        optionsByIntent.map { it[intent] }.distinctUntilChanged()

    override suspend fun selectOptions(intent: String, selection: FundingSelection) {
        runtimeProvider.runtime()
            .mapCatching { it.selectFundingOptions(intent, selection) }
            .logFailure("TrUAPI funding selection failed")
    }

    override suspend fun requestQuote(intent: String, ask: FundingQuoteAsk): Result<ULong> =
        runtimeProvider.runtime().mapCatching { it.getFundingQuote(intent, ask) }

    override suspend fun selectProvider(intent: String, providerId: ProductId, quoteId: String): Result<Boolean> =
        runtimeProvider.runtime().mapCatching { it.selectFundingProvider(intent, providerId, quoteId) }

    override suspend fun cancel(intent: String): Result<CancelOutcome> =
        runtimeProvider.runtime().mapCatching { it.cancelFunding(intent) }

    override suspend fun confirmCancel(intent: String, terms: CancelTerms?): Result<CancelConfirmation> =
        runtimeProvider.runtime().mapCatching { it.confirmFundingCancel(intent, terms) }

    override suspend fun acknowledge(intent: String): Result<Boolean> =
        runtimeProvider.runtime().mapCatching { it.acknowledgeFundingSession(intent) }

    override fun quoteLists(intent: String): Flow<FundingQuoteList> =
        quoteLists.asSharedFlow().filter { it.intent == intent }.map { it.quotes }

    override fun sessionChanges(): Flow<String> = changedSessions.asSharedFlow()

    private suspend fun readBack(notice: FundingChange) {
        val runtime = runtimeProvider.runtime().logFailure("Funding change read without a runtime").getOrNull() ?: return
        val intent = notice.intent
        when (notice.kind) {
            FundingChangeKind.SESSION -> {
                entries.value = runtime.fundingEntries().associateBy { it.session.intent }
                historyState.value = runtime.fundingHistory()
                intent?.let(changedSessions::tryEmit)
            }

            FundingChangeKind.OPTIONS -> if (intent != null) {
                val options = runtime.fundingOptions(intent)
                optionsByIntent.update { if (options == null) it - intent else it + (intent to options) }
            }

            FundingChangeKind.QUOTES -> if (intent != null) {
                runtime.fundingQuotes(intent)?.let { quoteLists.tryEmit(FundingQuoteListUpdate(intent, it)) }
            }

            // Funding resuming is when the core has loaded the ended sessions the history lists.
            FundingChangeKind.IN_PROGRESS -> historyState.value = runtime.fundingHistory()
        }
    }

    private data class FundingChange(val intent: String?, val kind: FundingChangeKind)

    private data class FundingQuoteListUpdate(val intent: String, val quotes: FundingQuoteList)

    private companion object {
        const val EVENT_BUFFER = 64
    }
}
