package io.paritytech.polkadotapp.feature_chats_impl.domain.addContact

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_chats_api.domain.model.contactOrNull
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.Chat
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ContactSearchResult

const val MAX_RECENT_CHATS = 5

class ContactSearchSections(
    val recents: List<Chat>,
    val allUsers: List<ContactSearchResult>,
) {
    val isEmpty: Boolean get() = recents.isEmpty() && allUsers.isEmpty()
}

fun composeContactSearchSections(
    query: String,
    recents: List<Chat>,
    blockedAccountIds: Set<AccountId>,
    allUsers: List<ContactSearchResult>,
): ContactSearchSections {
    val matchedRecents = recents
        .filter { chat -> chat.display.name.contains(query, ignoreCase = true) && chat.contactAccountId() !in blockedAccountIds }
        .take(MAX_RECENT_CHATS)
    val recentIds = matchedRecents.mapNotNull { it.contactAccountId() }.toSet()

    val matchedUsers = allUsers
        .filter { it.accountId !in blockedAccountIds && it.accountId !in recentIds }
        .sortedBy { it.username.getDisplayUsername().lowercase() }

    return ContactSearchSections(
        recents = matchedRecents,
        allUsers = matchedUsers,
    )
}

private fun Chat.contactAccountId(): AccountId? = id.contactOrNull()?.contactAccountId
