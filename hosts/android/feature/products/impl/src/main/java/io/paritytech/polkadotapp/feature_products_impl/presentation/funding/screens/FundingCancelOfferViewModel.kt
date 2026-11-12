package io.paritytech.polkadotapp.feature_products_impl.presentation.funding.screens

import dagger.assisted.Assisted
import dagger.assisted.AssistedFactory
import dagger.assisted.AssistedInject
import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.utils.launchUnit
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingService
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingCancelUiState
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingScreen
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingSessionFacts
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingSheet
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.WhileOnScreen
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.toDepositUiState
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.stateIn
import uniffi.truapi.CancelConfirmation
import uniffi.truapi.CancelTerms
import uniffi.truapi.FundingCancel
import io.paritytech.polkadotapp.common.R as RCommon

data class FundingCancelOfferUiState(
    val offer: FundingCancelUiState.Offered,
    val withdraw: Boolean,
    val isAnswering: Boolean,
)

/** The provider's terms for cancelling, which the user accepts as shown or declines. */
@HiltViewModel(assistedFactory = FundingCancelOfferViewModel.Factory::class)
class FundingCancelOfferViewModel @AssistedInject constructor(
    @Assisted private val sheet: FundingSheet,
    private val service: FundingService,
) : BaseViewModel() {
    private val isAnswering = MutableStateFlow(false)

    val state: StateFlow<FundingCancelOfferUiState?> =
        combine(sheet.flow.filterNotNull(), sheet.entry, isAnswering) { flow, entry, answering ->
            val deposit = flow.toDepositUiState(entry?.progress, FundingSessionFacts.of(entry), answering)
            (deposit.cancel as? FundingCancelUiState.Offered)?.let { FundingCancelOfferUiState(it, deposit.withdraw, answering) }
        }.stateIn(this, WhileOnScreen, null)

    fun onAccept() = answer(accept = true)

    fun onDecline() = answer(accept = false)

    private fun answer(accept: Boolean) = launchUnit {
        if (isAnswering.value) return@launchUnit
        isAnswering.value = true

        // Accepting names the terms on screen, so the core accepts nothing but the offer the user saw.
        val shown = sheet.entry.value?.session?.cancel as? FundingCancel.Offered
        val terms = if (accept) shown?.let { CancelTerms(fee = it.fee, refund = it.refund) } else null

        service.answerCancelOffer(sheet.intent, terms)
            .logFailure("Funding cancel answer failed")
            .onSuccess { leave(it.message()) }
            .onFailure { leave(RCommon.string.funding_error_cancel_failed) }

        isAnswering.value = false
    }

    private fun CancelConfirmation.message(): Int? = when (this) {
        CancelConfirmation.ACCEPTED -> RCommon.string.funding_cancel_accepted
        CancelConfirmation.DECLINED -> null
        CancelConfirmation.LAPSED -> RCommon.string.funding_cancel_offer_lapsed
        CancelConfirmation.NO_OFFER -> RCommon.string.funding_cancel_offer_gone
        CancelConfirmation.TOO_LATE -> RCommon.string.funding_cancel_too_late
    }

    private fun leave(message: Int?) {
        sheet.pop(FundingScreen.CANCEL_OFFER)
        message?.let(sheet::message)
    }

    @AssistedFactory
    interface Factory {
        fun create(sheet: FundingSheet): FundingCancelOfferViewModel
    }
}
