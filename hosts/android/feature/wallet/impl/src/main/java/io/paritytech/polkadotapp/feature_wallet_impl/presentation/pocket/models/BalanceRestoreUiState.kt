package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models

data class DigitalDollarCardDetailsUiState(
    val balanceRestore: BalanceRestoreUiState
)

sealed interface BalanceRestoreUiState {
    data object NotDetermined : BalanceRestoreUiState
    data object SendCash : BalanceRestoreUiState
    data class Restore(val inProgress: Boolean) : BalanceRestoreUiState
}
