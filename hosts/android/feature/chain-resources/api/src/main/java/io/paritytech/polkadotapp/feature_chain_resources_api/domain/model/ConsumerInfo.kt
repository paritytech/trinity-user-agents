package io.paritytech.polkadotapp.feature_chain_resources_api.domain.model

import io.paritytech.polkadotapp.common.domain.model.AccountEcdhKey
import io.paritytech.polkadotapp.common.domain.model.AccountId

class ConsumerInfo(
    val accountId: AccountId,
    val identifierKey: AccountEcdhKey,
    val fullUsername: String?,
    val liteUsername: String
) {
    val username: String
        get() = fullUsername ?: liteUsername
}
