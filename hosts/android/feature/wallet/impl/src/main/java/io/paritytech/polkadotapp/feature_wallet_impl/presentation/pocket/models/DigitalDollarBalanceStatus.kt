package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models

import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.TokenAmountModel

sealed interface DigitalDollarBalanceStatus {
    data object Syncing : DigitalDollarBalanceStatus

    data class PartlyReady(val amount: TokenAmountModel) : DigitalDollarBalanceStatus

    data object TotalOnly : DigitalDollarBalanceStatus
}
