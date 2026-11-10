package io.paritytech.polkadotapp.feature_chats_impl.domain.addContact

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.intoAccountId
import io.paritytech.polkadotapp.feature_chats_api.domain.middleware.bot.ChatPreview
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.Chat
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatAvatar
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatDisplay
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ChatSummaryBadge
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.ContactSearchResult
import io.paritytech.polkadotapp.feature_usernames_api.domain.model.Username
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class ContactSearchSectionsTest {
    @Test
    fun `recents match the query regardless of case`() {
        val sections = composeContactSearchSections(
            query = "AL",
            recents = listOf(chat("alice.01"), chat("bob.02")),
            blockedAccountIds = emptySet(),
            allUsers = emptyList(),
        )

        assertEquals(listOf("alice.01"), sections.recentNames())
    }

    @Test
    fun `recents show at most five matches in chat list order`() {
        val names = (1..7).map { "anna.0$it" }

        val sections = composeContactSearchSections(
            query = "anna",
            recents = names.map { chat(it) },
            blockedAccountIds = emptySet(),
            allUsers = emptyList(),
        )

        assertEquals(names.take(MAX_RECENT_CHATS), sections.recentNames())
    }

    @Test
    fun `a chat below the top five is still found by the query`() {
        val sections = composeContactSearchSections(
            query = "anna",
            recents = listOf("bob.01", "carl.02", "dave.03", "eve.04", "fred.05", "anna.06").map { chat(it) },
            blockedAccountIds = emptySet(),
            allUsers = emptyList(),
        )

        assertEquals(listOf("anna.06"), sections.recentNames())
    }

    @Test
    fun `a person in recents is not repeated in all users`() {
        val sections = composeContactSearchSections(
            query = "al",
            recents = listOf(chat("alice.01")),
            blockedAccountIds = emptySet(),
            allUsers = listOf(user("alice.01"), user("albert.05")),
        )

        assertEquals(listOf("alice.01"), sections.recentNames())
        assertEquals(listOf("albert.05"), sections.userNames())
    }

    @Test
    fun `blocked accounts are hidden from both sections`() {
        val sections = composeContactSearchSections(
            query = "al",
            recents = listOf(chat("alice.01"), chat("alex.03")),
            blockedAccountIds = setOf(accountId("alice.01")),
            allUsers = listOf(user("alice.01"), user("albert.05")),
        )

        assertEquals(listOf("alex.03"), sections.recentNames())
        assertEquals(listOf("albert.05"), sections.userNames())
    }

    @Test
    fun `nothing is left when every match is blocked`() {
        val sections = composeContactSearchSections(
            query = "al",
            recents = listOf(chat("alice.01")),
            blockedAccountIds = setOf(accountId("alice.01")),
            allUsers = listOf(user("alice.01")),
        )

        assertTrue(sections.isEmpty)
    }

    @Test
    fun `all users are sorted by username while recents keep their order`() {
        val sections = composeContactSearchSections(
            query = "a",
            recents = listOf(chat("zara.01"), chat("anna.02")),
            blockedAccountIds = emptySet(),
            allUsers = listOf(user("sam.05"), user("Aaron.06")),
        )

        assertEquals(listOf("zara.01", "anna.02"), sections.recentNames())
        assertEquals(listOf("Aaron.06", "sam.05"), sections.userNames())
    }

    private fun ContactSearchSections.recentNames() = recents.map { it.display.name }

    private fun ContactSearchSections.userNames() = allUsers.map { it.username.getDisplayUsername() }

    private fun chat(name: String) = Chat(
        id = ChatId.fromContact(accountId(name)),
        display = ChatDisplay(name, ChatAvatar.Account(name, name.encodeToByteArray())),
        preview = ChatPreview.EmptyChat,
        timestamp = 0L,
        unreadBadge = ChatSummaryBadge.None,
        hasUnseenReaction = false,
        customPreviewRenderer = null,
    )

    private fun user(name: String) = ContactSearchResult(
        username = Username.fromFullValue(name),
        accountId = accountId(name),
    )

    private fun accountId(name: String): AccountId = name.lowercase().encodeToByteArray().intoAccountId()
}
