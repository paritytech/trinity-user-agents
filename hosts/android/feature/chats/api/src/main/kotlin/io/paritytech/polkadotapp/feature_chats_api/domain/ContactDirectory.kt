package io.paritytech.polkadotapp.feature_chats_api.domain

import io.paritytech.polkadotapp.feature_chats_api.domain.model.Contact
import kotlinx.coroutines.flow.Flow

/**
 * The user's contacts, for a surface that lists them rather than chats in them.
 *
 * Blocked contacts are never included: a surface listing them would be offering
 * people the user refused.
 */
interface ContactDirectory {
    suspend fun getContacts(): List<Contact>

    /** The same list, emitted again whenever it changes. */
    fun observeContacts(): Flow<List<Contact>>
}
