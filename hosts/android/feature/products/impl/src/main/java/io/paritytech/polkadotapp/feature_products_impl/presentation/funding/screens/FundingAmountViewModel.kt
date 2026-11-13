package io.paritytech.polkadotapp.feature_products_impl.presentation.funding.screens

import dagger.assisted.Assisted
import dagger.assisted.AssistedFactory
import dagger.assisted.AssistedInject
import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingService
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingAmountUiState
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingFlowState
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingKey
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingScreen
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingSheet
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.WhileOnScreen
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.applying
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.toAmountUiState
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.mapLatest
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import uniffi.truapi.FundingRail
import java.math.BigDecimal
import kotlin.time.Duration.Companion.milliseconds

/** The amount and rail, checked with the core as the user types and quoted once they pause. */
@OptIn(ExperimentalCoroutinesApi::class)
@HiltViewModel(assistedFactory = FundingAmountViewModel.Factory::class)
class FundingAmountViewModel @AssistedInject constructor(
    @Assisted private val sheet: FundingSheet,
    private val service: FundingService,
) : BaseViewModel() {
    private var quoteJob: Job? = null

    private val issue = sheet.flow.filterNotNull()
        .map { Triple(it.units, it.options, it.list) to it }
        .distinctUntilChanged { old, new -> old.first == new.first }
        .mapLatest { (key, flow) ->
            key.first?.let { units -> service.amountIssue(units, flow.spendable?.let(flow.cash::units), sheet.intent) }
        }

    /** The sheet's picks with why the amount cannot go ahead, as the core last answered for it. */
    private val checked: StateFlow<FundingFlowState?> = combine(sheet.flow.filterNotNull(), issue) { flow, issue ->
        flow.withAmountIssue(issue)
    }.stateIn(this, WhileOnScreen, null)

    val state: StateFlow<FundingAmountUiState?> = checked.map { it?.toAmountUiState() }.stateIn(this, WhileOnScreen, null)

    fun onRailSelected(rail: FundingRail) {
        if (sheet.flow.value?.rail != rail) service.quote(null, sheet.intent)
        sheet.update { it.withRail(rail) }
        scheduleQuote()
    }

    fun onKey(key: FundingKey) {
        sheet.update { it.withAmountText(it.amountText.applying(key)) }
        scheduleQuote()
    }

    fun onPreset(value: BigDecimal) {
        sheet.update { it.withAmountText(FundingFlowState.textFor(value)) }
        scheduleQuote()
    }

    fun onContinue() {
        val current = checked.value?.takeIf { it.canContinue } ?: return

        when (current.rail) {
            FundingRail.CRYPTO -> sheet.push(FundingScreen.NETWORK)
            FundingRail.CARD, FundingRail.BANK -> {
                val pending = quoteJob?.isActive == true
                quoteJob?.cancel()
                if (pending || (!current.quoting && current.askedAmount != current.askAmount)) sheet.requote()
                sheet.push(FundingScreen.SUMMARY)
            }
        }
    }

    fun onClose() = sheet.close()

    /** Quotes the new amount once the user pauses, so a provider's limit shows before Continue. */
    private fun scheduleQuote() {
        quoteJob?.cancel()
        val current = sheet.flow.value ?: return
        if (current.rail == FundingRail.CRYPTO || current.amount.signum() <= 0) return

        quoteJob = launch {
            delay(REQUOTE_DELAY)
            sheet.requote()
        }
    }

    @AssistedFactory
    interface Factory {
        fun create(sheet: FundingSheet): FundingAmountViewModel
    }

    private companion object {
        val REQUOTE_DELAY = 600.milliseconds
    }
}
