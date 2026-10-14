package io.paritytech.polkadotapp.feature_transactions.api.presentation.outcome

import androidx.compose.runtime.Composable
import io.paritytech.polkadotapp.common.presentation.screens.BaseComposeFragment
import io.paritytech.polkadotapp.feature_transactions.api.presentation.outcome.compose.TransactionOutcomeScreen
import io.paritytech.polkadotapp.feature_transactions.api.presentation.outcome.compose.TransactionOutcomeUiConfig

abstract class TransactionOutcomeFragment<VM : TransactionOutcomeViewModel> :
    BaseComposeFragment<VM>() {
    abstract fun config(): TransactionOutcomeUiConfig

    @Composable
    override fun Screen() = TransactionOutcomeScreen(viewModel, config())
}
