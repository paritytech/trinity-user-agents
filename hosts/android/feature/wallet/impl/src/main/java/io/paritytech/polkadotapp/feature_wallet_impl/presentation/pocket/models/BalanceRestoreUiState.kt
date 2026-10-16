package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models

data class DigitalDollarCardDetailsUiState(
    val balanceRestore: BalanceRestoreUiState
)

sealed interface BalanceRestoreUiState {
    data object NotDetermined : BalanceRestoreUiState

    /** The Get and Send actions; Get only in builds that can fund. */
    data class SendCash(val canGetCash: Boolean) : BalanceRestoreUiState
    data class Restore(val inProgress: Boolean) : BalanceRestoreUiState
}
