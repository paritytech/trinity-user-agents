package io.paritytech.polkadotapp.feature_balances_impl.domain.model

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.util.amountFromPlanks
import io.paritytech.polkadotapp.feature_balances_api.domain.model.TokenBalance
import kotlin.text.compareTo

class RealTokenBalance(
    override val token: Chain.Asset,
    override val free: Balance,
    override val reserved: Balance,
    override val frozen: Balance,
    private val edCountingMode: EDCountingMode,
    private val transferableMode: TransferableMode,
) : TokenBalance {
    override val transferable
        get() = transferableMode.transferableBalance(free, frozen, reserved)

    override val balanceCountedTowardsED
        get() = edCountingMode.balanceCountedTowardsEd(free, reserved)

    override val total: Balance
        get() = free + reserved

    override fun reservable(existentialDeposit: Balance): Balance {
        return transferableMode.reservableBalance(
            free = free,
            frozen = frozen,
            ed = existentialDeposit
        )
    }

    override fun canReserve(amount: Balance): Boolean {
        return free >= amount
    }

    override fun toString(): String {
        val components = listOf(
            "free" to free,
            "reserved" to reserved,
            "frozen" to frozen,
            "transferable" to transferable
        )
            .joinToString { (label, value) ->
                val amount = token.amountFromPlanks(value)
                "$label=$amount"
            }

        return "RealTokenBalance(token=${token.symbol},$components)"
    }
}

fun TokenBalance.Companion.legacy(
    token: Chain.Asset,
    free: Balance,
    reserved: Balance,
    frozen: Balance
): TokenBalance {
    return RealTokenBalance(
        token = token,
        free = free,
        reserved = reserved,
        frozen = frozen,
        edCountingMode = EDCountingMode.TOTAL,
        transferableMode = TransferableMode.LEGACY
    )
}

fun TokenBalance.Companion.simple(
    token: Chain.Asset,
    balance: Balance
): TokenBalance {
    return RealTokenBalance(
        token = token,
        free = balance,
        reserved = Balance.ZERO,
        frozen = Balance.ZERO,
        edCountingMode = EDCountingMode.TOTAL,
        transferableMode = TransferableMode.LEGACY
    )
}
