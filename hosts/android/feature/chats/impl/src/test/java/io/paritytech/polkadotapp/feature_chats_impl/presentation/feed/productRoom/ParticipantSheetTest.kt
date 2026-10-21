package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.productRoom

import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatParticipantEntryModel
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ChatParticipantModel
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

    private fun entry(id: String, participant: ChatParticipantModel) = ChatParticipantEntryModel(id, participant)
}
