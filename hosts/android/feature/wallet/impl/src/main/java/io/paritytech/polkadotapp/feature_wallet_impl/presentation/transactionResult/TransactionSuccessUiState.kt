package io.paritytech.polkadotapp.feature_wallet_impl.presentation.transactionResult

data class TransactionSuccessUiState(
    val finality: Finality?,
) {
    enum class Finality {
        PENDING,
        CONFIRMED,
    }
}
