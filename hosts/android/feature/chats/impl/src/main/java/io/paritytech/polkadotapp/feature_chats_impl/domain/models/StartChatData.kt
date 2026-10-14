package io.paritytech.polkadotapp.feature_chats_impl.domain.models

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.X25519PublicKey
import io.paritytech.polkadotapp.feature_account_api.domain.model.SharedSecretDerivationDomain
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ContactOrigin
import io.paritytech.polkadotapp.feature_usernames_api.domain.model.Username

sealed class StartChatData {
    data class NewChat(
        val contactAccountId: AccountId,
        val username: Username?,
        val avatar: String?,
        val chatKey: X25519PublicKey,
        val sharedSecretDerivationDomain: SharedSecretDerivationDomain,
        val ourMetaAccountId: Long,
        val origin: ContactOrigin
    ) : StartChatData()

    data class ExistingChat(
        val contactAccountId: AccountId,
    ) : StartChatData()
}
