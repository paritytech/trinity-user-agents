package io.paritytech.polkadotapp.feature_chats_api.domain.model

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.X25519PublicKey

typealias ContactAccountId = AccountId

data class ContactDevice(
    val contactAccountId: ContactAccountId,
    val statementAccountId: AccountId,
    val encryptionPublicKey: X25519PublicKey,
)
