package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom

import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatActionHandlerModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatParticipantEntryModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatParticipantMenuModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatParticipantModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatProductActionModel
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class ParticipantSheetTest {
    @Test
    fun `each kind has its host entry`() {
        val account = ByteArray(32) { 0 }.toDataByteArray()

        assertEquals(
            listOf(ParticipantSheetEntry.OpenProduct("https://weather.dot")),
            ParticipantSheet.builtIns("product", entry = null, roomProductAddress = "https://weather.dot"),
        )
        assertEquals(
            listOf(ParticipantSheetEntry.OpenProduct("https://other.dot")),
            ParticipantSheet.builtIns("o", entry("o", ChatParticipantModel.Product("other.dot")), roomProductAddress = null),
        )
        val copy = ParticipantSheet.builtIns("l", entry("l", ChatParticipantModel.LitePerson(account)), roomProductAddress = null).single()
        assertTrue((copy as ParticipantSheetEntry.CopyAddress).address.startsWith("5"))
        assertEquals(emptyList<ParticipantSheetEntry>(), ParticipantSheet.builtIns("user", entry = null, roomProductAddress = null))
        assertEquals(emptyList<ParticipantSheetEntry>(), ParticipantSheet.builtIns("p", entry("p", ChatParticipantModel.Person(1)), null))
        assertEquals(emptyList<ParticipantSheetEntry>(), ParticipantSheet.builtIns("c", entry("c", ChatParticipantModel.Contact(account)), null))
        assertEquals(emptyList<ParticipantSheetEntry>(), ParticipantSheet.builtIns("g", entry("g", ChatParticipantModel.Custom("G", null)), null))
    }

    @Test
    fun `custom drops the host entry, extend keeps it, and a destructive entry goes last`() {
        val builtIns = listOf(ParticipantSheetEntry.OpenProduct("https://other.dot"))
        val actions = listOf(action("kick", destructive = true), action("mute"))

        assertEquals(
            listOf(product("mute", destructive = false), product("kick", destructive = true)),
            ParticipantSheet.entries(builtIns, ChatParticipantMenuModel.Custom(actions), icons = emptyMap()),
        )
        assertEquals(
            builtIns + listOf(product("mute", destructive = false), product("kick", destructive = true)),
            ParticipantSheet.entries(builtIns, ChatParticipantMenuModel.ExtendBuiltIn(actions), icons = emptyMap()),
        )
        assertEquals(builtIns, ParticipantSheet.entries(builtIns, menu = null, icons = emptyMap()))
    }

    private fun entry(id: String, participant: ChatParticipantModel) = ChatParticipantEntryModel(id, participant)

    private fun action(id: String, destructive: Boolean = false) =
        ChatProductActionModel(id, id, icon = null, destructive = destructive, handler = ChatActionHandlerModel.Callback)

    private fun product(id: String, destructive: Boolean) = ParticipantSheetEntry.Product(id, id, icon = null, isDestructive = destructive)
}
