package io.paritytech.polkadotapp.feature_statement_store_api.domain.models

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.X25519PublicKey

sealed interface SessionAccount {
    val accountId: AccountId
    val pin: String?

    data class Local(
        override val accountId: AccountId,
        override val pin: String?
    ) : SessionAccount

    data class Remote(
        override val accountId: AccountId,
        override val pin: String?,
        val publicKey: X25519PublicKey
    ) : SessionAccount
}
