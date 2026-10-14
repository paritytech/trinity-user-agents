package io.paritytech.polkadotapp.feature_fund_impl.presentation.fund.terms.compose

import androidx.compose.runtime.Immutable
import io.paritytech.polkadotapp.feature_prices_api.presentation.mapper.model.FiatAmountModel
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.AssetDisplay
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.TokenAmountModel
import kotlinx.collections.immutable.ImmutableList
import kotlin.time.Duration
import kotlin.time.Duration.Companion.seconds

@Immutable
data class FundUiState(
    val doneEnabled: Boolean,
    val assetDisplay: AssetDisplay,
    val chainName: String,
    val minimumSendAmount: TokenAmountModel,
    val fundingAddress: String,
    val fee: FiatAmountModel,
    val conversion: ConversionModel,
    val operations: ImmutableList<FundingOperation>,
)

@Immutable
data class ConversionModel(
    val from: TokenAmountModel,
    val to: TokenAmountModel,
)

data class FundingOperation(
    val id: String,
    val status: Status,
    val conversion: Pair<TokenAmountModel, TokenAmountModel>,
) {
    @Immutable
    sealed class Status {
        data class InProgress(val countdownTime: Duration) : Status()

        data object Done : Status()

        data object Failure : Status()
    }
}

fun List<FundingOperation>.getStatus(): FundingOperation.Status {
    if (any { it.status is FundingOperation.Status.Failure }) return FundingOperation.Status.Failure
    if (all { it.status is FundingOperation.Status.Done }) return FundingOperation.Status.Done
    return FundingOperation.Status.InProgress(0L.seconds)
}
