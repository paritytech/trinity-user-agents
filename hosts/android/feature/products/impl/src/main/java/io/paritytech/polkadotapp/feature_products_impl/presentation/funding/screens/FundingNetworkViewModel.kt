package io.paritytech.polkadotapp.feature_products_impl.presentation.funding.screens

import dagger.assisted.Assisted
import dagger.assisted.AssistedFactory
import dagger.assisted.AssistedInject
import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingNetworkRow
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingScreen
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingSheet
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.WhileOnScreen
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.toNetworkRows
import kotlinx.collections.immutable.ImmutableList
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn

@HiltViewModel(assistedFactory = FundingNetworkViewModel.Factory::class)
class FundingNetworkViewModel @AssistedInject constructor(
    @Assisted private val sheet: FundingSheet,
) : BaseViewModel() {
    val state: StateFlow<ImmutableList<FundingNetworkRow>?> = sheet.flow.map { it?.toNetworkRows() }.stateIn(this, WhileOnScreen, null)

    fun onNetworkChosen(id: String) {
        sheet.update { it.withNetwork(id) }
        sheet.push(FundingScreen.TOKEN)
    }

    @AssistedFactory
    interface Factory {
        fun create(sheet: FundingSheet): FundingNetworkViewModel
    }
}
