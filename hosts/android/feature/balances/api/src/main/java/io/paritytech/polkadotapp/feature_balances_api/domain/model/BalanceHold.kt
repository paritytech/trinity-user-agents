package io.paritytech.polkadotapp.feature_balances_api.domain.model

import io.paritytech.polkadotapp.chains.network.binding.Balance

data class BalanceHoldId(val module: String, val reason: String)

class BalanceHold(val id: BalanceHoldId, val amount: Balance)
