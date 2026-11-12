package io.paritytech.polkadotapp.feature_products_impl.presentation.funding.screens

import dagger.assisted.Assisted
import dagger.assisted.AssistedFactory
import dagger.assisted.AssistedInject
import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.presentation.clipboard.ClipboardService
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingDepositUiState
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingScreen
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingSessionFacts
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingSheet
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.WhileOnScreen
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.toDepositUiState
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.stateIn
import uniffi.truapi.FundingCancel
import io.paritytech.polkadotapp.common.R as RCommon

/** Where to send a bank or crypto top-up, then how the session goes. */
@HiltViewModel(assistedFactory = FundingDepositViewModel.Factory::class)
class FundingDepositViewModel @AssistedInject constructor(
    @Assisted private val sheet: FundingSheet,
    private val clipboard: ClipboardService,
) : BaseViewModel() {
    val state: StateFlow<FundingDepositUiState?> = combine(sheet.flow.filterNotNull(), sheet.entry) { flow, entry ->
        flow.toDepositUiState(entry?.progress, FundingSessionFacts.of(entry), isCancelling = false)
    }.stateIn(this, WhileOnScreen, null)

    fun onCopy(value: String) {
        clipboard.setPrimaryClip(value)
        sheet.message(RCommon.string.funding_deposit_copied)
    }

    fun onCancel() = sheet.push(FundingScreen.CANCEL_CONFIRM)

    /** Shows the provider's cancel offer again, after the user went back from it. */
    fun onReviewOffer() {
        if (sheet.entry.value?.session?.cancel is FundingCancel.Offered) sheet.push(FundingScreen.CANCEL_OFFER)
    }

    fun onClose() = sheet.close()

    @AssistedFactory
    interface Factory {
        fun create(sheet: FundingSheet): FundingDepositViewModel
    }
}
