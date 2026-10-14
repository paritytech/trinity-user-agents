package io.paritytech.polkadotapp.feature_chats_impl.domain

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_chats_api.domain.ContactDisplayInfo
import io.paritytech.polkadotapp.feature_chats_api.domain.ContactDisplayProvider
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ContactAvatar
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ContactsRepository
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatAvatar
import javax.inject.Inject

class RealContactDisplayProvider @Inject constructor(
    private val contactsRepository: ContactsRepository,
    private val chatEngine: ChatEngine
) : ContactDisplayProvider {
    override suspend fun getContactDisplay(contactAccountId: AccountId): ContactDisplayInfo? {
        val contact = contactsRepository.getContact(contactAccountId) ?: return null
        val chatDisplay = chatEngine.getContactChatDisplay(contact)
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
