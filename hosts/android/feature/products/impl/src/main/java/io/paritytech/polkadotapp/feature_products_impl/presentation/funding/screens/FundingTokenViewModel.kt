package io.paritytech.polkadotapp.feature_products_impl.presentation.funding.screens

import dagger.assisted.Assisted
import dagger.assisted.AssistedFactory
import dagger.assisted.AssistedInject
import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.utils.launchUnit
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_products_impl.domain.funding.FundingService
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingScreen
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingSheet
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.FundingTokensUiState
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.WhileOnScreen
import io.paritytech.polkadotapp.feature_products_impl.presentation.funding.toTokensUiState
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import uniffi.truapi.FundingDirection

@HiltViewModel(assistedFactory = FundingTokenViewModel.Factory::class)
class FundingTokenViewModel @AssistedInject constructor(
    @Assisted private val sheet: FundingSheet,
    private val service: FundingService,
) : BaseViewModel() {
    val state: StateFlow<FundingTokensUiState?> = sheet.flow.map { it?.toTokensUiState() }.stateIn(this, WhileOnScreen, null)

    /** Value in goes straight to the deposit screen, which starts on the best quote once every provider answered. */
    fun onTokenChosen(symbol: String) {
        val direction = sheet.flow.value?.direction ?: return
        sheet.update { it.withAsset(symbol) }
        sheet.requote()

        if (direction == FundingDirection.IN) {
            launchUnit { service.start(sheet.intent, null).logFailure("Funding auto-start failed") }
            sheet.push(FundingScreen.DEPOSIT)
        } else {
            sheet.push(FundingScreen.SUMMARY)
        }
    }

    @AssistedFactory
    interface Factory {
        fun create(sheet: FundingSheet): FundingTokenViewModel
    }
}
