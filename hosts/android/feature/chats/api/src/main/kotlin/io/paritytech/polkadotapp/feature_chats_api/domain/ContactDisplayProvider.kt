package io.paritytech.polkadotapp.feature_chats_api.domain

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ContactAvatar

interface ContactDisplayProvider {
    suspend fun getContactDisplay(contactAccountId: AccountId): ContactDisplayInfo?
}

data class ContactDisplayInfo(
    val username: String,
    val avatar: ContactAvatar
)
