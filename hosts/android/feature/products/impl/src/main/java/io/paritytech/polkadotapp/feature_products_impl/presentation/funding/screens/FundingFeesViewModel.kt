package io.paritytech.polkadotapp.feature_products_impl.presentation.funding.screens

import dagger.assisted.Assisted
import dagger.assisted.AssistedFactory
import dagger.assisted.AssistedInject
import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingFeesUiState
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingSheet
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.WhileOnScreen
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.toFeesUiState
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn

@HiltViewModel(assistedFactory = FundingFeesViewModel.Factory::class)
class FundingFeesViewModel @AssistedInject constructor(
    @Assisted sheet: FundingSheet,
) : BaseViewModel() {
    val state: StateFlow<FundingFeesUiState?> = sheet.flow.map { it?.toFeesUiState() }.stateIn(this, WhileOnScreen, null)

    @AssistedFactory
    interface Factory {
        fun create(sheet: FundingSheet): FundingFeesViewModel
    }
}
