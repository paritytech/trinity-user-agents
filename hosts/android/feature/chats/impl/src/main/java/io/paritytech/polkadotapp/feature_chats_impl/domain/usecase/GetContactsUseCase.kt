package io.paritytech.polkadotapp.feature_chats_impl.domain.usecase

import io.paritytech.polkadotapp.feature_chats_api.domain.usecase.GetContactsUseCase
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ContactsRepository
import javax.inject.Inject

class RealGetContactsUseCase @Inject constructor(
    private val contactsRepository: ContactsRepository
) : GetContactsUseCase {
    override suspend operator fun invoke() = contactsRepository.getContacts()
}
