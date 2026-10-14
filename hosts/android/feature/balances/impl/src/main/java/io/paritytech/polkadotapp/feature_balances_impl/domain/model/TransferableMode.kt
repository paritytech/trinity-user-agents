package io.paritytech.polkadotapp.feature_balances_impl.domain.model

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.chains.network.binding.max
import io.paritytech.polkadotapp.feature_balances_api.domain.model.TokenBalance

/**
 * Defines what logic to use when calculating transferable balance from [TokenBalance.free], [TokenBalance.frozen] and [TokenBalance.reserved]
 */
enum class TransferableMode {
    /**
     * Legacy calculation mode used before introduction of Holds and Freezes
     * Does not allow for Locks and Reserves to overlap
     */
    LEGACY {
        override fun transferableBalance(free: Balance, frozen: Balance, reserved: Balance): Balance {
            return (free - frozen).atLeastZero()
        }

        override fun reservableBalance(free: Balance, frozen: Balance, ed: Balance): Balance {
            return free - max(ed, frozen)
        }
    },

    /**
     * New calculation mode introduced by Holds and Freezes
     * Allows for Locks and Reserves to overlap
     */
    HOLDS_AND_FREEZES {
        override fun transferableBalance(free: Balance, frozen: Balance, reserved: Balance): Balance {
            val freeCannotDropBelow = (frozen - reserved).atLeastZero()

            return (free - freeCannotDropBelow).atLeastZero()
        }

        override fun reservableBalance(free: Balance, frozen: Balance, ed: Balance): Balance {
            return free - ed
        }
    };

    abstract fun transferableBalance(free: Balance, frozen: Balance, reserved: Balance): Balance

    abstract fun reservableBalance(free: Balance, frozen: Balance, ed: Balance): Balance
}
