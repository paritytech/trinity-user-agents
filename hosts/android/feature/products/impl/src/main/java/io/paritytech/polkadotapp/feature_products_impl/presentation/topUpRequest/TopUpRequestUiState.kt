package io.paritytech.polkadotapp.feature_products_impl.presentation.topUpRequest

import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.TokenAmountModel

sealed interface TopUpRequestUiState {
    val productId: String

    /** Nothing was claimed, and nothing more will be tried. */
    data class Failure(override val productId: String) : TopUpRequestUiState

    /** The claim succeeded but [creditedAmount] funds were accepted instead of [requestedAmount]. */
    data class PartialPayment(
        override val productId: String,
        val requestedAmount: TokenAmountModel,
        val creditedAmount: TokenAmountModel,
    ) : TopUpRequestUiState
}
