package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models

import androidx.compose.runtime.Immutable
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.TokenAmountModel

/** The balance as the domain holds it, before Ready and Clearing fold it into two buckets. */
@Immutable
data class CoinageBalanceBreakdownUiModel(
    val availablePrivate: TokenAmountModel,
    val gainingPrivacy: TokenAmountModel,
    val pending: TokenAmountModel,
    val canSpendGainingPrivacy: Boolean
)
