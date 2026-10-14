package io.paritytech.polkadotapp.feature_swap_api.domain.model

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.utils.Fraction
import io.paritytech.polkadotapp.common.utils.graph.Path
import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount

data class SwapQuoteArgs(
    val assetIn: Chain.Asset,
    val assetOut: Chain.Asset,
    val amount: Balance,
    val swapDirection: SwapDirection,
)

class SwapFeeArgs(
    val assetIn: Chain.Asset,
    val slippage: Fraction,
    val executionPath: Path<SegmentExecuteArgs>,
    val direction: SwapDirection,
    val firstSegmentFees: Chain.Asset,
    val sender: MetaAccount,
    val recipient: AccountId,
)

class SegmentExecuteArgs(
    val quotedSwapEdge: QuotedEdge<SwapGraphEdge>,
)
