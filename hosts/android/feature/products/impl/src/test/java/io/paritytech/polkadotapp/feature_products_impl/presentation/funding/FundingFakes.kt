package io.paritytech.polkadotapp.feature_products_impl.presentation.funding

import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FakeFundingCore
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingCash
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingEntry
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingEvent
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingFrameRequest
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingProviderBrand
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingService
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.takeWhile
import uniffi.truapi.CancelConfirmation
import uniffi.truapi.CancelOutcome
import uniffi.truapi.CancelTerms
import uniffi.truapi.FundingAmountIssue
import uniffi.truapi.FundingDirection
import uniffi.truapi.FundingFrameOutcome
import uniffi.truapi.FundingOptions
import uniffi.truapi.FundingQuoteList
import uniffi.truapi.FundingSelection
import uniffi.truapi.FundingSession
import uniffi.truapi.ProductId
import uniffi.truapi.U128

/** The funding service as plain state: each session's entries are pushed by the test, and every call is recorded. */
class FakeFundingService : FundingService {
    /** Session "intent" as it changes; `null` ends what follows it. */
    val entry = MutableSharedFlow<FundingEntry?>(replay = 1)
    val calls = mutableListOf<String>()
    var startResult: Result<Unit> = Result.success(Unit)
    var cancelResult: Result<CancelOutcome> = Result.success(CancelOutcome.REQUESTED)
    var answerResult: Result<CancelConfirmation> = Result.success(CancelConfirmation.ACCEPTED)
    var answeredTerms: CancelTerms? = null

    override fun entries(): Flow<List<FundingEntry>> = emptyFlow()

    override fun entryState(id: String): Flow<FundingEntry> = entry.takeWhile { it != null }.map { requireNotNull(it) }

    override suspend fun resume(): Result<Unit> = Result.success(Unit)

    override suspend fun open(direction: FundingDirection, amount: U128?): Result<String?> = Result.success(null)

    override fun select(selection: FundingSelection, id: String) {
        calls += "select ${selection.rail} ${selection.asset} ${selection.country}"
    }

    override fun quote(amount: U128?, id: String) {
        calls += "quote $amount"
    }

    override suspend fun amountIssue(amount: U128, spendable: U128?, id: String): FundingAmountIssue? = null

    override suspend fun start(id: String, provider: ProductId?): Result<Unit> =
        startResult.also { calls += "start ${provider?.value}" }

    override fun stopStarting(id: String) {
        calls += "stopStarting"
    }

    override fun leave(id: String) {
        calls += "leave"
    }

    override suspend fun cancel(id: String): Result<CancelOutcome> = cancelResult.also { calls += "cancel" }

    override suspend fun answerCancelOffer(id: String, terms: CancelTerms?): Result<CancelConfirmation> {
        answeredTerms = terms
        calls += "answer"
        return answerResult
    }

    override suspend fun acknowledge(id: String) = Unit

    override val events: Flow<FundingEvent> = emptyFlow()

    override fun openProviderFrame(request: FundingFrameRequest) = Unit

    override fun answerFrame(request: FundingFrameRequest, outcome: FundingFrameOutcome) = Unit
}

/** The sheet as plain state, recording where its screens send it. */
class FakeFundingSheet(state: FundingFlowState, entry: FundingEntry?) : FundingSheet {
    override val intent = "intent"
    override val entry = MutableStateFlow(entry)
    override val flow = MutableStateFlow<FundingFlowState?>(state)
    override val brands: StateFlow<Map<ProductId, FundingProviderBrand>> = MutableStateFlow(emptyMap())
    val actions = mutableListOf<String>()
    val messages = mutableListOf<Int>()

    override fun update(transform: (FundingFlowState) -> FundingFlowState) {
        flow.value = flow.value?.let(transform)
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
    val cash = FundingCash(precision = 6)

    fun entry(
        session: FundingSession? = FakeFundingCore.session("intent"),
        options: FundingOptions? = FakeFundingCore.options(ramp),
        quotes: FundingQuoteList? = null,
        isPresented: Boolean = true,
    ) = FundingEntry.of(
        id = "intent",
        fallbackDirection = FundingDirection.IN,
        fallbackAmount = null,
        session = session,
        progress = session?.let { FakeFundingCore.progress() },
        summary = null,
        options = options,
        quotes = quotes,
        staleQuotes = emptyMap(),
        isQuoting = false,
        isPresented = isPresented,
    )

    fun flow(options: FundingOptions? = FakeFundingCore.options(ramp), amount: String = "") = FundingFlowState.initial(
        direction = FundingDirection.IN,
        cash = cash,
        format = testCashFormat(),
        spendable = null,
        options = options,
        amount = null,
        country = FundingCountry(code = "DE", name = "Germany", currencyCode = "EUR", currencyName = "Euro"),
    ).withAmountText(amount)
}
