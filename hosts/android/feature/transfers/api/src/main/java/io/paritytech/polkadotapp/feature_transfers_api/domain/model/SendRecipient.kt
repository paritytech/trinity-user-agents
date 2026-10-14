package io.paritytech.polkadotapp.feature_transfers_api.domain.model

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.FullChainAssetId
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.Timestamp

class SendRecipient(
    val accountId: AccountId,
    val fullChainAssetId: FullChainAssetId,
    val createdAt: Timestamp = System.currentTimeMillis(),
)
