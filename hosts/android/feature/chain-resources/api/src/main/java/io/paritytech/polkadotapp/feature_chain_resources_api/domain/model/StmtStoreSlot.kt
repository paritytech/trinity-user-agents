package io.paritytech.polkadotapp.feature_chain_resources_api.domain.model

import io.paritytech.polkadotapp.common.domain.model.AccountId

sealed interface StmtStoreSlot {
    object Free : StmtStoreSlot

    data class Occupied(
        val accountId: AccountId,
        val sinceSeconds: Long,
    ) : StmtStoreSlot
}
