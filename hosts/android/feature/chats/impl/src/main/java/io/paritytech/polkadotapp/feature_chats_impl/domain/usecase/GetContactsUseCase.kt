package io.paritytech.polkadotapp.feature_chats_impl.domain.usecase

import io.paritytech.polkadotapp.feature_chats_api.domain.PersonChatContacts
import io.paritytech.polkadotapp.feature_chats_api.domain.PersonChatMode
import io.paritytech.polkadotapp.feature_chats_api.domain.model.hasEstablishedChat
import io.paritytech.polkadotapp.feature_chats_api.domain.usecase.AddressBookContact
import io.paritytech.polkadotapp.feature_chats_api.domain.usecase.GetContactsUseCase
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ContactsRepository
import javax.inject.Inject

class RealGetContactsUseCase @Inject constructor(
    private val contactsRepository: ContactsRepository,
    private val personChatMode: PersonChatMode,
    private val personChatContacts: PersonChatContacts,
) : GetContactsUseCase {
    // Coins through the chat product come with M9; until then its contacts are listed but not paid through chat.
    override suspend operator fun invoke(): List<AddressBookContact> =
        if (personChatMode.isProductChat()) {
            personChatContacts.getContacts().map {
                AddressBookContact(it.accountId, it.username, it.blocked, established = true, paysViaChat = false)
            }
        } else {
            contactsRepository.getContacts().map {
                AddressBookContact(
                    accountId = it.accountId,
                    username = it.username,
                    blocked = it.isBlocked,
                    established = it.hasEstablishedChat(),
                    paysViaChat = it.hasEstablishedChat(),
                )
            }
        }
}
