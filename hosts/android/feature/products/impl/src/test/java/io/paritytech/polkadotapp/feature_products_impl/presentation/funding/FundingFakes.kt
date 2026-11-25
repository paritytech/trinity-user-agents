package io.paritytech.polkadotapp.feature_products_impl.presentation.funding

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FakeFundingCore
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingEntry
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingFrameEvent
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingFrameRequest
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingIntentId
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingProviderBrand
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingProviderFrames
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingQuotes
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingSessions
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.toBalanceOrNull
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.takeWhile
import uniffi.truapi.CancelConfirmation
import uniffi.truapi.CancelOutcome
import uniffi.truapi.CancelTerms
import uniffi.truapi.FundingAmountIssue
import uniffi.truapi.FundingDirection
import uniffi.truapi.FundingFrameOutcome
import uniffi.truapi.FundingOptions
import uniffi.truapi.FundingProgress
import uniffi.truapi.FundingQuoteList
import uniffi.truapi.FundingSelection
import uniffi.truapi.FundingSession
import uniffi.truapi.ProductId
import kotlin.time.Instant

/** The funding parts as plain state: each session's entries are pushed by the test, and every call is recorded. */
class FakeFunding : FundingSessions, FundingQuotes, FundingProviderFrames {
    /** Session "intent" as it changes; `null` ends what follows it. */
    val entry = MutableSharedFlow<FundingEntry?>(replay = 1)
    val calls = mutableListOf<String>()
    var startResult: Result<Unit> = Result.success(Unit)
    var cancelResult: Result<CancelOutcome> = Result.success(CancelOutcome.REQUESTED)
    var answerResult: Result<CancelConfirmation> = Result.success(CancelConfirmation.ACCEPTED)
    var answeredTerms: CancelTerms? = null

    override fun entries(): Flow<List<FundingEntry>> = listed

    override fun entryState(id: FundingIntentId): Flow<FundingEntry> = entry.takeWhile { it != null }.map { requireNotNull(it) }

    override fun shown(id: FundingIntentId) = Unit

    override suspend fun isAvailable(): Result<Boolean> = Result.success(true)

    override suspend fun resume(): Result<Unit> = Result.success(Unit)

    override suspend fun open(direction: FundingDirection, amount: Balance?): Result<FundingIntentId?> = Result.success(null)

    override fun select(selection: FundingSelection, id: FundingIntentId) {
        calls += "select ${selection.rail} ${selection.asset} ${selection.country}"
    }

    override fun quote(amount: Balance?, id: FundingIntentId) {
        calls += "quote ${amount?.value}"
    }

    override suspend fun amountIssue(amount: Balance, spendable: Balance?, id: FundingIntentId): Result<FundingAmountIssue?> = Result.success(null)

    override suspend fun start(id: FundingIntentId, provider: ProductId?): Result<Unit> =
        startResult.also { calls += "start ${provider?.value}" }

    override fun leave(id: FundingIntentId) {
        calls += "leave"
    }

    override suspend fun cancel(id: FundingIntentId): Result<CancelOutcome> = cancelResult.also { calls += "cancel" }

    override suspend fun answerCancelOffer(id: FundingIntentId, terms: CancelTerms?): Result<CancelConfirmation> {
        answeredTerms = terms
        calls += "answer"
        return answerResult
    }

    override suspend fun acknowledge(id: FundingIntentId) = Unit

    val requestQueue = MutableSharedFlow<FundingEntry>(extraBufferCapacity = 16)

    override val presentationRequests: Flow<FundingEntry> = requestQueue

    val eventQueue = MutableSharedFlow<FundingFrameEvent>(extraBufferCapacity = 16)

    override val events: Flow<FundingFrameEvent> = eventQueue

    /** Sessions whose provider waits for its screen to be opened. */
    val waitingFrames = mutableSetOf<FundingIntentId>()

    /** Every session listed, as [entries] gives them. */
    val listed = MutableStateFlow<List<FundingEntry>>(emptyList())

    override suspend fun open(intentId: FundingIntentId): Result<Boolean> =
        Result.success(waitingFrames.remove(intentId)).also { calls += "openProviderFrame ${intentId.value}" }

    override suspend fun isWaiting(intentId: FundingIntentId): Result<Boolean> = Result.success(intentId in waitingFrames)

    override fun answer(request: FundingFrameRequest, outcome: FundingFrameOutcome) = Unit
}

/** The sheet as plain state, recording where its screens send it. */
class FakeFundingSheet(state: FundingFlowState, entry: FundingEntry?) : FundingSheet {
    override val intentId = FundingIntentId("intent")
    override val entry = MutableStateFlow(entry)
    override val flow = MutableStateFlow<FundingFlowState?>(state)
    override val brands: StateFlow<Map<ProductId, FundingProviderBrand>> = MutableStateFlow(emptyMap())
    override val now = MutableStateFlow(Instant.fromEpochMilliseconds(0))
    val picks = MutableStateFlow(FundingPicks.NONE)
    val actions = mutableListOf<String>()
    val messages = mutableListOf<Int>()

    override fun update(transform: (FundingPicks) -> FundingPicks) {
        picks.value = transform(picks.value)
        flow.value = flow.value?.let(picks.value::applyTo)
    }

    override fun requote() {
        actions += "requote"
    }

    override fun push(screen: FundingScreen) {
        actions += "push $screen"
    }

    override fun pop(screen: FundingScreen) {
        actions += "pop $screen"
    }

    override fun close() {
        actions += "close"
    }

    override fun message(text: Int) {
        messages += text
    }
}

object FundingFixtures {
    val ramp: ProductId = ProductId.parse("ramp.dot").getOrThrow()
    const val CASH_PRECISION = 6

    fun entry(
        session: FundingSession? = FakeFundingCore.session("intent"),
        options: FundingOptions? = FakeFundingCore.options(ramp),
        quotes: FundingQuoteList? = null,
        isPresented: Boolean = true,
        progress: FundingProgress? = session?.let { FakeFundingCore.progress() },
    ) = FundingEntry.of(
        id = FundingIntentId("intent"),
        fallbackDirection = FundingDirection.IN,
        fallbackAmount = null,
        session = session,
        progress = progress,
        summary = null,
        options = options,
        askedAmount = quotes?.amount?.toBalanceOrNull(),
        quotes = quotes,
        quotesFailed = false,
        isPresented = isPresented,
    )

    fun flow(options: FundingOptions? = FakeFundingCore.options(ramp), amount: String = "") = FundingFlowState.initial(
        direction = FundingDirection.IN,
        cashPrecision = CASH_PRECISION,
        format = testCashFormat(),
        spendable = null,
        options = options,
        amount = null,
        country = FundingCountry(code = "DE", name = "Germany", currencyCode = "EUR", currencyName = "Euro"),
    ).withAmountText(amount)
}
