package io.paritytech.polkadotapp.feature_products_impl.presentation.funding

import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.presentation.clipboard.ClipboardService
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.common.presentation.loading.asLoaded
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.utils.launchUnit
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_products_api.domain.funding.FundingOverlayOutcome
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingFlowInteractor
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingFlowState
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingKey
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingOverlayContext
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingOverlayContexts
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingProviderBrand
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.applying
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.isOpen
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.isPending
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.quote
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.toCore
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import uniffi.truapi.FundingProgress
import uniffi.truapi.FundingQuoteRow
import uniffi.truapi.FundingRail
import java.math.BigDecimal
import javax.inject.Inject
import kotlin.time.Clock
import kotlin.time.Duration.Companion.milliseconds
import kotlin.time.Duration.Companion.seconds
import io.paritytech.polkadotapp.common.R as RCommon
import uniffi.truapi.FundingDirection as CoreFundingDirection

/** Whether the session has been handed to a provider. */
data class FundingStartState(
    val isStarting: Boolean,
    val failed: Boolean,
    val started: Boolean,
)

@HiltViewModel
class FundingViewModel @Inject constructor(
    private val context: FundingOverlayContext,
    private val contexts: FundingOverlayContexts,
    private val interactor: FundingFlowInteractor,
    private val router: ProductsRouter,
    private val clipboard: ClipboardService,
) : BaseViewModel() {
    private companion object {
        val REQUOTE_DELAY = 600.milliseconds
        val TICK = 1.seconds
        val QUOTED_SCREENS = setOf(FundingScreen.SUMMARY, FundingScreen.PROVIDERS)
        val DEPOSIT_POLL = 3.seconds
    }

    private val intent = context.request.intent
    private val flow = MutableStateFlow<FundingFlowState?>(null)
    private val path = MutableStateFlow<List<FundingScreen>>(emptyList())
    private val start = MutableStateFlow(FundingStartState(isStarting = false, failed = false, started = false))
    private val progress = MutableStateFlow<FundingProgress?>(null)
    private var quoteJob: Job? = null
    private var startsWhenQuoted = false

    private val brands = MutableStateFlow<Map<String, FundingProviderBrand>>(emptyMap())
    private val countryQuery = MutableStateFlow("")
    private val now = MutableStateFlow(Clock.System.now())
    private val countries by lazy { interactor.countries() }
    private val detectedCountry by lazy { interactor.detectedCountry() }

    val state: StateFlow<LoadingState<FundingSheetUiState>> = combine(
        flow.filterNotNull(),
        path,
        combine(start, progress) { start, progress -> start to progress },
        brands,
        combine(countryQuery, now) { query, now -> query to now },
    ) { flow, path, (start, progress), brands, (query, now) ->
        FundingSheetUiState(
            screen = path.lastOrNull() ?: FundingScreen.AMOUNT,
            amount = flow.toAmountUiState(),
            summary = flow.toSummaryUiState(brands, start),
            fees = flow.toFeesUiState(),
            country = flow.toCountryUiState(countries, detectedCountry, query),
            providers = flow.toProvidersUiState(brands, now),
            networks = flow.toNetworkRows(),
            tokens = flow.toTokensUiState(),
            deposit = flow.toDepositUiState(progress, start),
        ).asLoaded()
    }.stateIn(this, SharingStarted.Eagerly, LoadingState.Loading)

    init {
        launch { interactor.quoteRows(intent).collect { onQuoteRow(it) } }
        launch { interactor.sessionChanges(intent).collect { refreshSession() } }
        launchUnit {
            val initial = initialState()
            flow.value = initial
            loadBrands(initial)
        }
        launch { path.map { it.lastOrNull() in QUOTED_SCREENS }.distinctUntilChanged().collectLatest { if (it) tickWhileQuoted() } }
        launch { path.map { it.lastOrNull() == FundingScreen.DEPOSIT }.distinctUntilChanged().collectLatest { if (it) followDeposit() } }
    }

    fun onNetworkChosen(id: String) {
        flow.update { it?.withNetwork(id) }
        push(FundingScreen.TOKEN)
    }

    /** Value in goes straight to the deposit screen, which starts with the best quote once every provider answered. */
    fun onTokenChosen(symbol: String) {
        val current = flow.value ?: return
        flow.value = current.withAsset(symbol)
        requestQuote()

        if (current.direction == CoreFundingDirection.IN) {
            startsWhenQuoted = true
            push(FundingScreen.DEPOSIT)
        } else {
            push(FundingScreen.SUMMARY)
        }
    }

    fun onCopy(value: String) {
        clipboard.setPrimaryClip(value)
        showMessage(RCommon.string.funding_deposit_copied)
    }

    fun onCancelTopUp() = push(FundingScreen.CANCEL_CONFIRM)

    fun onConfirmCancel() = launchUnit {
        interactor.cancel(intent).logFailure("Funding cancel failed")
        onClose()
    }

    fun onRailSelected(rail: FundingRail) {
        flow.update { it?.withRail(rail) }
        scheduleQuote()
    }

    fun onKey(key: FundingKey) {
        flow.update { it?.withAmountText(it.amountText.applying(key)) }
        scheduleQuote()
    }

    fun onPreset(value: BigDecimal) {
        flow.update { it?.withAmountText(FundingFlowState.textFor(value)) }
        scheduleQuote()
    }

    fun onContinueFromAmount() {
        val current = flow.value?.takeIf { it.canContinue } ?: return

        when (current.rail) {
            FundingRail.CRYPTO -> push(FundingScreen.NETWORK)
            FundingRail.CARD, FundingRail.BANK -> {
                if (current.ask != current.currentAsk()) requestQuote()
                push(FundingScreen.SUMMARY)
            }
        }
    }

    fun onOpenFees() {
        if (flow.value?.selectedQuote != null) push(FundingScreen.FEES)
    }

    fun onOpenCountry() {
        countryQuery.value = ""
        push(FundingScreen.COUNTRY)
    }

    fun onOpenProviders() = push(FundingScreen.PROVIDERS)

    fun onCountryQueryChanged(query: String) {
        countryQuery.value = query
    }

    fun onCountryChosen(code: String) {
        val chosen = countries.firstOrNull { it.code == code } ?: return
        flow.update { it?.withCountry(chosen) }
        pop()
        requestQuote()
    }

    fun onProviderChosen(providerId: String) {
        flow.update { it?.withChosenProvider(providerId) }
        showMessage(RCommon.string.funding_provider_changed)
        pop()
    }

    /** Hands the session to the provider whose quote is on screen, then answers `Started`. */
    fun onStart() {
        val current = flow.value ?: return
        val providerId = current.selectedProviderId ?: return
        if (start.value.isStarting || start.value.started) return

        start.value = FundingStartState(isStarting = true, failed = false, started = false)
        launchUnit {
            val selected = interactor.selectProvider(intent, providerId, current.rows[providerId]?.quote?.quoteId)
                .logFailure("Funding provider selection failed")
                .getOrDefault(false)

            start.value = FundingStartState(isStarting = false, failed = !selected, started = selected)
            if (selected) didStart(current.rail)
        }
    }

    fun onBack() {
        val leavesStartedDeposit = start.value.started && path.value.lastOrNull() == FundingScreen.DEPOSIT
        if (path.value.isEmpty() || leavesStartedDeposit) onClose() else pop()
    }

    /** Before `Started` leaving is the answer the core discards the session on; after it the session runs on. */
    fun onClose() {
        quoteJob?.cancel()
        context.answer(FundingOverlayOutcome.DISMISSED)
        closeSheet()
    }

    override fun onCleared() {
        super.onCleared()
        context.answer(FundingOverlayOutcome.DISMISSED)
        context.markClosed()
        contexts.remove(intent)
    }

    private fun closeSheet() = launchUnit { router.closeFundingOverlay(intent) }

    private suspend fun initialState(): FundingFlowState {
        val cash = interactor.cash()
        val request = context.request

        return FundingFlowState.initial(
            direction = request.direction.toCore(),
            cash = cash,
            spendable = interactor.spendable(cash),
            candidates = interactor.candidates(intent),
            amount = request.amount?.let { cash.decimal(it.value.toString()) },
            country = interactor.detectedCountry(),
        )
    }

    private fun push(screen: FundingScreen) {
        path.update { it + screen }
    }

    private fun pop() {
        path.update { it.dropLast(1) }
    }

    private fun onQuoteRow(row: FundingQuoteRow) {
        val updated = flow.value?.receiving(row) ?: return
        flow.value = updated

        if (startsWhenQuoted && updated.allAnswered) {
            startsWhenQuoted = false
            onStart()
        }
    }

    /** The core's view of the session moved on: read it again, and leave once it is over. */
    private suspend fun refreshSession() {
        progress.value = interactor.progress(intent)
        val ended = interactor.session(intent)?.stage?.isOpen == false
        if (start.value.started && ended) closeSheet()
    }

    private suspend fun followDeposit() {
        while (true) {
            if (start.value.started) refreshSession()
            delay(DEPOSIT_POLL)
        }
    }

    private fun didStart(rail: FundingRail) {
        context.answer(FundingOverlayOutcome.STARTED)
        launchUnit { refreshSession() }

        when (rail) {
            FundingRail.CARD -> closeSheet()
            FundingRail.BANK, FundingRail.CRYPTO -> if (path.value.lastOrNull() != FundingScreen.DEPOSIT) push(FundingScreen.DEPOSIT)
        }
    }

    private fun loadBrands(state: FundingFlowState) {
        state.candidates.forEach { candidate ->
            launchUnit {
                val brand = interactor.brand(candidate.providerId)
                brands.update { it + (candidate.providerId to brand) }
            }
        }
    }

    /** Drives the countdown and asks again once the prices on screen expire. */
    private suspend fun tickWhileQuoted() {
        while (true) {
            now.value = Clock.System.now()
            refreshIfExpired()
            delay(TICK)
        }
    }

    private fun refreshIfExpired() {
        val current = flow.value ?: return
        val expiry = current.quoteExpiry ?: return
        if (expiry <= now.value && current.rows.values.none { it.isPending }) requestQuote()
    }

    /** Quotes the new amount once the user pauses, so a provider's limit shows before Continue. */
    private fun scheduleQuote() {
        quoteJob?.cancel()
        val current = flow.value ?: return
        if (current.rail == FundingRail.CRYPTO || current.amount.signum() <= 0) return

        quoteJob = launch {
            delay(REQUOTE_DELAY)
            requestQuote()
        }
    }

    private fun requestQuote() {
        val ask = flow.value?.currentAsk() ?: return
        flow.update { it?.asking(ask) }

        launchUnit {
            interactor.requestQuote(intent, ask).logFailure("Funding quote request failed")
        }
    }
}
