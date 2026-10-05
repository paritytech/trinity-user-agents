package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models

import androidx.compose.runtime.Immutable
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.TokenAmountModel
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.coins.CoinageScene
import kotlinx.collections.immutable.ImmutableList

@Immutable
data class CoinageUiState(
    val tokensState: TokensState,
    val autoFundAvailable: Boolean,
    val fundInProgress: Boolean,
    val actionsEnabled: Boolean,
    /**
     * Gates the log-sharing button inside the debug card, on top of the card's own gate: sharing a log file
     * is testnet support tooling, and a build that shows the debug card need not offer it.
     */
    val shareLogsEnabled: Boolean,
    val detailsVisible: Boolean
) {
    /**
     * The two figures partition [totalBalance] exactly, and [coins] is a picture of the same two — one coin
     * per holding, in the same two runs — so the numbers and the strip are read off one classification and
     * cannot contradict each other.
     */
    @Immutable
    data class TokensState(
        val totalBalance: TokenAmountModel,
        /** Free to use without giving up any privacy. */
        val readyBalance: TokenAmountModel,
        /** Everything else: gaining privacy, in flight, or due to be recycled before it can be used. */
        val clearingBalance: TokenAmountModel,
        /** Already in display order: Clearing first, then largest denomination, then least fungible. */
        val coins: ImmutableList<CoinageScene.Coin>,
        val breakdown: CoinageBalanceBreakdownUiModel
    )
}
