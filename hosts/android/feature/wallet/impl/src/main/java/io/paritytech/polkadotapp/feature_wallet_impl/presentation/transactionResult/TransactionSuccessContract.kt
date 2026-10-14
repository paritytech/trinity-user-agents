package io.paritytech.polkadotapp.feature_wallet_impl.presentation.transactionResult

import io.paritytech.polkadotapp.feature_transactions.api.presentation.outcome.TransactionOutcomeContract
import kotlinx.coroutines.flow.StateFlow

interface TransactionSuccessContract : TransactionOutcomeContract {
    val state: StateFlow<TransactionSuccessUiState>
}
