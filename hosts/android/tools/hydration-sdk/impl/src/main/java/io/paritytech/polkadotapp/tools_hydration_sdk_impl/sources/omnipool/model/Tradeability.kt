package io.paritytech.polkadotapp.tools_hydration_sdk_impl.sources.omnipool.model

import io.paritytech.polkadotapp.chains.network.binding.bindNumber
import io.paritytech.polkadotapp.common.data.substrate.castToStruct
import java.math.BigInteger

@JvmInline
value class Tradeability(val value: BigInteger) {
    companion object {
        // / Asset is allowed to be sold into omnipool
        val SELL = 0b0000_0001.toBigInteger()

        // / Asset is allowed to be bought into omnipool
        val BUY = 0b0000_0010.toBigInteger()
    }

    fun canBuy(): Boolean = flagEnabled(BUY)

    fun canSell(): Boolean = flagEnabled(SELL)

    private fun flagEnabled(flag: BigInteger) = value and flag == flag
}

fun bindTradeability(value: Any?): Tradeability {
    val asStruct = value.castToStruct()

    return Tradeability(bindNumber(asStruct["bits"]))
}
