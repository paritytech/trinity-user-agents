package io.paritytech.polkadotapp.feature_balances_api.domain.model

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.common.domain.model.AccountId

class DepositEvent(
    val destination: AccountId,
    val amount: Balance,
)
