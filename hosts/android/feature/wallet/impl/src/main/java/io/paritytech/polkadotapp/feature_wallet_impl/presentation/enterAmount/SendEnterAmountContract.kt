package io.paritytech.polkadotapp.feature_wallet_impl.presentation.enterAmount

import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.common.presentation.validation.ValidationMixin
import io.paritytech.polkadotapp.common.utils.progressStallReport.StalenessReportDisplay
import kotlinx.coroutines.flow.StateFlow

interface SendEnterAmountContract {
    val state: StateFlow<LoadingState<SendEnterAmountUiState>>

    val sendValidationMixin: ValidationMixin

    val stalenessReport: StalenessReportDisplay

    fun onConfirmClick()

    fun onBackClick()

    fun onNewInput(value: String)
}
