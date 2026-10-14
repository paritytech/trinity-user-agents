package io.paritytech.polkadotapp.feature_usernames_api.domain.model

import io.paritytech.polkadotapp.common.domain.model.AccountId

data class FoundUser(
    val accountId: AccountId,
    val username: Username,
    val onChainData: OnChainData?,
)

data class OnChainData(
    val blockHash: String,
    val blockNumber: Long,
    val blockIndex: Long,
    val eventIndex: Long,
)
