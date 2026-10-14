package io.paritytech.polkadotapp.feature_balances_api.domain.model

import io.paritytech.polkadotapp.chains.network.binding.BlockHash

data class AccountBalanceUpdate(
    val updatedAt: BlockHash?, // null In case update block hash is unknown
    val balance: TokenBalance,
)
