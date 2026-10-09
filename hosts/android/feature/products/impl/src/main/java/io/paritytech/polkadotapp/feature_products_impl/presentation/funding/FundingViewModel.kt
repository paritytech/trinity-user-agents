package io.paritytech.polkadotapp.feature_products_impl.presentation.funding

import dagger.hilt.android.lifecycle.HiltViewModel
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
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.applying
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.toCore
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import uniffi.truapi.FundingRail
import java.math.BigDecimal
import javax.inject.Inject
import kotlin.time.Duration.Companion.milliseconds

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
) : BaseViewModel() {
    private companion object {
        val REQUOTE_DELAY = 600.milliseconds
    }

    private val intent = context.request.intent
    private val flow = MutableStateFlow<FundingFlowState?>(null)
    private val path = MutableStateFlow<List<FundingScreen>>(emptyList())
    private val start = MutableStateFlow(FundingStartState(isStarting = false, failed = false, started = false))
    private var quoteJob: Job? = null

    val state: StateFlow<LoadingState<FundingSheetUiState>> = combine(flow.filterNotNull(), path, start) { flow, path, _ ->
        FundingSheetUiState(
            screen = path.lastOrNull() ?: FundingScreen.AMOUNT,
            amount = flow.toAmountUiState(),
        ).asLoaded()
    }.stateIn(this, SharingStarted.Eagerly, LoadingState.Loading)

    init {
        launch { interactor.quoteRows(intent).collect { row -> flow.update { it?.receiving(row) } } }
        launchUnit { flow.value = initialState() }
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

    fun onBack() {
        if (path.value.isEmpty()) onClose() else path.update { it.dropLast(1) }
    }

    /** Before `Started` leaving is the answer the core discards the session on; after it the session runs on. */
    fun onClose() {
        quoteJob?.cancel()
        context.answer(FundingOverlayOutcome.DISMISSED)
        router.back()
    }

    override fun onCleared() {
        super.onCleared()
        context.answer(FundingOverlayOutcome.DISMISSED)
        contexts.remove(intent)
    }

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
