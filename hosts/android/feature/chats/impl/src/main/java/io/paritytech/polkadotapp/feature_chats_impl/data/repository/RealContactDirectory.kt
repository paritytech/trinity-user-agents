package io.paritytech.polkadotapp.feature_chats_impl.data.repository

import io.paritytech.polkadotapp.feature_chats_api.domain.ContactDirectory
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Contact
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import javax.inject.Inject

/**
 * The contact list as a surface outside chat sees it: everyone the user has a
 * thread with, minus the ones they blocked.
 */
class RealContactDirectory @Inject constructor(
    private val contacts: ContactsRepository,
) : ContactDirectory {
    override suspend fun getContacts(): List<Contact> =
        contacts.getContacts().filterNot { it.isBlocked }

    override fun observeContacts(): Flow<List<Contact>> =
        contacts.subscribeContacts().map { list -> list.filterNot { it.isBlocked } }
}
