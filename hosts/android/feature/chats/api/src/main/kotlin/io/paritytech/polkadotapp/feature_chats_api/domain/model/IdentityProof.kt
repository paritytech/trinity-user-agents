package io.paritytech.polkadotapp.feature_chats_api.domain.model

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.DataByteArray

data class IdentityProof(
    val identityAccountId: AccountId,
    val proof: DataByteArray,
)
