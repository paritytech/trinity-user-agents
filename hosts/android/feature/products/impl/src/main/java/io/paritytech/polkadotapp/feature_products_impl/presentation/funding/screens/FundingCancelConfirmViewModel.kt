package io.paritytech.polkadotapp.feature_products_impl.presentation.funding.screens

import dagger.assisted.Assisted
import dagger.assisted.AssistedFactory
import dagger.assisted.AssistedInject
import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.utils.launchUnit
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingSessions
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingDepositUiState
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
import uniffi.truapi.CancelOutcome
import io.paritytech.polkadotapp.common.R as RCommon

// Before money moves a cancel ends the session; after, the provider answers it with a refusal or an offer.
@HiltViewModel(assistedFactory = FundingCancelConfirmViewModel.Factory::class)
class FundingCancelConfirmViewModel @AssistedInject constructor(
    @Assisted private val sheet: FundingSheet,
    private val sessions: FundingSessions,
) : BaseViewModel() {
    private val isCancelling = MutableStateFlow(false)

    val state: StateFlow<FundingDepositUiState?> =
        combine(sheet.flow.filterNotNull(), sheet.entry, isCancelling, sheet.now) { flow, entry, cancelling, now ->
            flow.toDepositUiState(entry?.progress, FundingSessionFacts.of(entry), cancelling, now)
        }.stateIn(this, WhileOnScreen, null)

    fun onConfirm() = launchUnit {
        if (isCancelling.value) return@launchUnit
        isCancelling.value = true

        sessions.cancel(sheet.intentId)
            .logFailure("Funding cancel failed")
            .onSuccess { outcome ->
                when (outcome) {
                    CancelOutcome.CANCELLED -> sheet.close()
                    CancelOutcome.REQUESTED, CancelOutcome.PENDING -> leave(RCommon.string.funding_cancel_requested)
                    CancelOutcome.TOO_LATE -> leave(RCommon.string.funding_cancel_too_late)
                    CancelOutcome.REFUSED -> leave(RCommon.string.funding_error_cancel_refused)
                }
            }
            .onFailure { leave(RCommon.string.funding_error_cancel_failed) }

        isCancelling.value = false
    }

    private fun leave(message: Int) {
        sheet.pop(FundingScreen.CANCEL_CONFIRM)
        sheet.message(message)
    }

    @AssistedFactory
    interface Factory {
        fun create(sheet: FundingSheet): FundingCancelConfirmViewModel
    }
}
