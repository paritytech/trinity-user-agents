package io.paritytech.polkadotapp.feature_tokens_api.presentation.amountinput

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.feature_balances_api.presentation.provider.AvailableBalanceProvider
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.RoundPrecision
import io.paritytech.polkadotapp.feature_tokens_api.presentation.model.TokenAmountModel
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.Flow

interface AmountInputMixin {
    interface Factory {
        fun create(
            coroutineScope: CoroutineScope,
            asset: suspend () -> Chain.Asset,
            roundPrecision: RoundPrecision,
            availableBalanceProvider: AvailableBalanceProvider,
        ): AmountInputMixin
    }

    val value: Flow<AmountInputValue>

    val availableBalance: Flow<TokenAmountModel>

    fun setNewInput(amountInput: AmountInput)
}

context(scope: ComputationalScope)
fun AmountInputMixin.Factory.create(
    asset: suspend () -> Chain.Asset,
    roundPrecision: RoundPrecision,
    availableBalanceProvider: AvailableBalanceProvider,
): AmountInputMixin =
    create(
        coroutineScope = scope,
        roundPrecision = roundPrecision,
        asset = asset,
        availableBalanceProvider = availableBalanceProvider
    )

data class AmountInput(val input: String, val origin: Origin) {
    enum class Origin {
        /**
         * Input has been originated from user keyboard
         */
        USER,

        /**
         * Input has been originated from max button related behavior event
         */
        MAX_BUTTON,

        /**
         * Input has been originated by other internal reason
         */
        INTERNAL
    }
}

class AmountInputValue(
    val input: AmountInput,
    val amount: TokenAmountModel,
)

fun AmountInput.Origin.isMaxButton(): Boolean {
    return this == AmountInput.Origin.MAX_BUTTON
}

fun AmountInput.Origin.isInternal(): Boolean {
    return this == AmountInput.Origin.INTERNAL
}

fun AmountInput.Origin.isUser(): Boolean {
    return this == AmountInput.Origin.USER
}
