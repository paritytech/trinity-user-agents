package io.paritytech.polkadotapp.feature_fund_impl.presentation.fund.terms

import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.feature_fund_impl.presentation.fund.terms.compose.FundUiState
import kotlinx.coroutines.flow.StateFlow

interface FundContract {
    val state: StateFlow<LoadingState<FundUiState>>

    fun doneClicked()

    fun copyAddressClicked(address: String)
}
