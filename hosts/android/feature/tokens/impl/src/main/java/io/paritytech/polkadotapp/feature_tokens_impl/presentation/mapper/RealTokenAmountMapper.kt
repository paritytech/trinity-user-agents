package io.paritytech.polkadotapp.feature_tokens_impl.presentation.mapper

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainAssetWithAmount
import io.paritytech.polkadotapp.chains.util.amountFromPlanks
import io.paritytech.polkadotapp.feature_tokens_api.presentation.mapper.TokenAmountMapper
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.TokenAmountModel
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.TokenSymbolAppearance
import java.math.BigDecimal
import javax.inject.Inject

class RealTokenAmountMapper @Inject constructor() : TokenAmountMapper {
    companion object {
        private val DIGITAL_DOLLAR_TICKERS = listOf("pUSD", "HOLLAR", "dUSD")
    }

    override fun mapFrom(source: ChainAssetWithAmount): TokenAmountModel {
        val amount = source.amount
        val asset = source.chainAsset
        val appearance = if (DIGITAL_DOLLAR_TICKERS.contains(asset.symbol)) {
            TokenSymbolAppearance.DigitalDollar
        } else {
            TokenSymbolAppearance.Symbol(asset.symbol)
        }

        return RealTokenAmountModel(
            amount = amount.amountFromPlanks(asset.precision),
            appearance = appearance
        )
    }

    private data class RealTokenAmountModel(
        override val amount: BigDecimal,
        override val appearance: TokenSymbolAppearance,
    ) : TokenAmountModel
}
