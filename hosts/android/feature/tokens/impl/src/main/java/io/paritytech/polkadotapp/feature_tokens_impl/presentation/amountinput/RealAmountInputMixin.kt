package io.paritytech.polkadotapp.feature_tokens_impl.presentation.amountinput

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.withAmount
import io.paritytech.polkadotapp.common.utils.filterDigits
import io.paritytech.polkadotapp.feature_balances_api.presentation.provider.AvailableBalanceProvider
import io.paritytech.polkadotapp.feature_tokens_api.presentation.amountinput.AmountInput
import io.paritytech.polkadotapp.feature_tokens_api.presentation.amountinput.AmountInputMixin
import io.paritytech.polkadotapp.feature_tokens_api.presentation.amountinput.AmountInputValue
import io.paritytech.polkadotapp.feature_tokens_api.presentation.mapper.TokenAmountMapper
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.RoundPrecision
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.TokenAmountModel
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.mapNotNull
import java.math.BigDecimal
import javax.inject.Inject

class RealAmountInputMixin(
    private val assetProvider: suspend () -> Chain.Asset,
    private val roundPrecision: RoundPrecision,
    availableBalanceProvider: AvailableBalanceProvider,
    private val tokenAmountMapper: TokenAmountMapper,
) : AmountInputMixin {
    private val input =
        MutableStateFlow(AmountInput(input = "", AmountInput.Origin.INTERNAL))

    override val availableBalance: Flow<TokenAmountModel> =
        availableBalanceProvider.maxAvailableBalance
            .map {
                tokenAmountMapper.mapFrom(assetProvider().withAmount(it))
            }

    override val value = input
        .mapNotNull { input ->
            val asset = assetProvider()
            val amount = (input.input.toBigDecimalOrNull() ?: BigDecimal.ZERO)

            val tokenAmount = tokenAmountMapper.mapFrom(asset.withAmount(amount))
            AmountInputValue(input, tokenAmount)
        }

    override fun setNewInput(amountInput: AmountInput) {
        val filteredInput = amountInput.input.filterDigits(roundPrecision.digits)
        input.value = amountInput.copy(input = filteredInput)
    }
}

class AmountInputMixinFactory @Inject constructor(
    private val tokenAmountMapper: TokenAmountMapper,
) : AmountInputMixin.Factory {
    override fun create(
        coroutineScope: CoroutineScope,
        asset: suspend () -> Chain.Asset,
        roundPrecision: RoundPrecision,
        availableBalanceProvider: AvailableBalanceProvider,
    ): AmountInputMixin {
        return RealAmountInputMixin(
            assetProvider = asset,
            roundPrecision = roundPrecision,
            availableBalanceProvider = availableBalanceProvider,
            tokenAmountMapper = tokenAmountMapper
        )
    }
}
