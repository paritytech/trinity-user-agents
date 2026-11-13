package io.paritytech.polkadotapp.feature_products_impl.presentation.funding.screens

import dagger.assisted.Assisted
import dagger.assisted.AssistedFactory
import dagger.assisted.AssistedInject
import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingScreen
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingSheet
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingTokensUiState
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.WhileOnScreen
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.toTokensUiState
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn

@HiltViewModel(assistedFactory = FundingTokenViewModel.Factory::class)
class FundingTokenViewModel @AssistedInject constructor(
    @Assisted private val sheet: FundingSheet,
) : BaseViewModel() {
    val state: StateFlow<FundingTokensUiState?> = sheet.flow.map { it?.toTokensUiState() }.stateIn(this, WhileOnScreen, null)

    /** On to the summary, as for card and bank: the user sees the provider, fees and total, and starts from there. */
    fun onTokenChosen(symbol: String) {
        if (sheet.flow.value == null) return
        sheet.update { it.withAsset(symbol) }
        sheet.requote()
        sheet.push(FundingScreen.SUMMARY)
    }

    @AssistedFactory
    interface Factory {
        fun create(sheet: FundingSheet): FundingTokenViewModel
    }
}
