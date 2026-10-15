package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatResourceModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatParticipantEntryModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatParticipantModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductChatQueryException
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductParticipantsPage
import org.junit.Assert.assertEquals
import org.junit.Assert.assertThrows
import org.junit.Test
import uniffi.truapi.ChatParticipant
import uniffi.truapi.ChatParticipantEntry
import uniffi.truapi.ChatResource
import uniffi.truapi.ContactHandle
import uniffi.truapi.ProductChatParticipantsQueryItem

class ChatParticipantsMappingTest {
    private val account = ByteArray(32) { it.toByte() }

    @Test
    fun `each kind of participant crosses the bridge`() {
        val entries = listOf(
            ChatParticipantEntry("p", ChatParticipant.Person(7u)),
            ChatParticipantEntry("l", ChatParticipant.LitePerson(account)),
            ChatParticipantEntry("c", ChatParticipant.Contact(ContactHandle(account))),
            ChatParticipantEntry("o", ChatParticipant.Product("other.dot")),
            ChatParticipantEntry("g", ChatParticipant.Custom("Ada", ChatResource.Inline("image/png", byteArrayOf(1)))),
        )

        val page = ProductChatParticipantsQueryItem.Page(entries, next = "2").toPage()

        assertEquals(
            ProductParticipantsPage(
                listOf(
                    ChatParticipantEntryModel("p", ChatParticipantModel.Person(7)),
                    ChatParticipantEntryModel("l", ChatParticipantModel.LitePerson(account.toDataByteArray())),
                    ChatParticipantEntryModel("c", ChatParticipantModel.Contact(account.toDataByteArray())),
                    ChatParticipantEntryModel("o", ChatParticipantModel.Product("other.dot")),
                    ChatParticipantEntryModel(
                        "g",
                        ChatParticipantModel.Custom("Ada", ChatResourceModel.Inline("image/png", byteArrayOf(1).toDataByteArray())),
                    ),
                ),
                next = "2",
            ),
            page,
        )
        assertEquals(entries.map { it.participant.javaClass }, page.participants.map { it.toCore().participant.javaClass })
    }

    @Test
    fun `the count and the resolved participants come from their own answers`() {
        assertEquals(4, ProductChatParticipantsQueryItem.Count(4u).toCount())
        assertEquals(
            listOf(ChatParticipantEntryModel("p", ChatParticipantModel.Person(1))),
            ProductChatParticipantsQueryItem.Resolved(listOf(ChatParticipantEntry("p", ChatParticipant.Person(1u)))).toResolved(),
        )
    }

    @Test
    fun `an answer to another question fails`() {
        assertThrows(ProductChatQueryException.Failed::class.java) { ProductChatParticipantsQueryItem.Count(4u).toPage() }
    }
}
