package io.paritytech.polkadotapp.feature_chats_impl.domain

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_chats_api.domain.ContactDisplayInfo
import io.paritytech.polkadotapp.feature_chats_api.domain.ContactDisplayProvider
import io.paritytech.polkadotapp.feature_chats_api.domain.PersonChatContacts
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ContactAvatar
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatDisplay.ChatDisplayGenerator
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatAvatar
import javax.inject.Inject

class RealContactDisplayProvider @Inject constructor(
    private val personChatContacts: PersonChatContacts,
    private val chatDisplayGenerator: ChatDisplayGenerator,
) : ContactDisplayProvider {
    override suspend fun getContactDisplay(contactAccountId: AccountId): ContactDisplayInfo? {
        // The contacts of the chat product are the contacts.
        val chatDisplay = personChatContacts.getContacts().firstOrNull { it.accountId == contactAccountId }?.let {
            chatDisplayGenerator.generateAccountChatDisplay(it.accountId, it.username, avatarUrl = null)
        } ?: return null
        return ContactDisplayInfo(
            username = chatDisplay.name,
            avatar = chatDisplay.avatar.toContactAvatar()
        )
    }
}

private fun ChatAvatar.toContactAvatar(): ContactAvatar = when (this) {
    is ChatAvatar.Account -> ContactAvatar.Account(name, themeSeed)
    is ChatAvatar.Url -> ContactAvatar.Url(url)
}
