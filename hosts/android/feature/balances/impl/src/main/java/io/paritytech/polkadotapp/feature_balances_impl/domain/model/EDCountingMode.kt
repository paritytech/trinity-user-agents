package io.paritytech.polkadotapp.feature_balances_impl.domain.model

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.feature_balances_api.domain.model.TokenBalance

/**
 * Defines what logic to use when calculating balance counted towards Existential Deposit
 * from [TokenBalance.free], [TokenBalance.frozen] and [TokenBalance.reserved]
 */
enum class EDCountingMode {
    /**
     * Everything is countable towards ED
     */
    TOTAL {
        override fun balanceCountedTowardsEd(free: Balance, reserved: Balance): Balance {
            return free + reserved
        }
    },

    /**
     * Only free balance is countable towards ED
     */
    FREE {
        override fun balanceCountedTowardsEd(free: Balance, reserved: Balance): Balance {
            return free
        }
    };

    abstract fun balanceCountedTowardsEd(free: Balance, reserved: Balance): Balance
}
