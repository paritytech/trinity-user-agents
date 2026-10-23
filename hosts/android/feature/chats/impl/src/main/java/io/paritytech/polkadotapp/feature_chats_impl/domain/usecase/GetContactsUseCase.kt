package io.paritytech.polkadotapp.feature_chats_impl.domain.usecase

import io.paritytech.polkadotapp.feature_chats_api.domain.PersonChatContacts
import io.paritytech.polkadotapp.feature_chats_api.domain.usecase.AddressBookContact
import io.paritytech.polkadotapp.feature_chats_api.domain.usecase.GetContactsUseCase
import javax.inject.Inject

class RealGetContactsUseCase @Inject constructor(
    private val personChatContacts: PersonChatContacts,
) : GetContactsUseCase {
    // The chat product lists the contacts whose chat is established, and blocked contacts.
    override suspend operator fun invoke(): List<AddressBookContact> =
        personChatContacts.getContacts().map {
            AddressBookContact(it.accountId, it.username, it.blocked, established = true)
        }
}
