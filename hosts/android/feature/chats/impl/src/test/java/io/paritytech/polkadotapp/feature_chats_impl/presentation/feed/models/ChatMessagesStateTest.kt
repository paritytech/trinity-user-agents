package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models

import org.junit.Assert.assertEquals
import org.junit.Test

class ChatMessagesStateTest {
    // The room screen shows the empty-room text of the product only for a room that is empty, not for one that did not load.
    @Test
    fun `only a ready room without messages is empty`() {
        val emptyByReadiness = ChatRoomReadinessUiState.entries.associateWith { ChatMessagesState(readiness = it).isEmptyRoom }

        assertEquals(
            mapOf(
                ChatRoomReadinessUiState.LOADING to false,
                ChatRoomReadinessUiState.READY to true,
                ChatRoomReadinessUiState.UNAVAILABLE to false,
                ChatRoomReadinessUiState.FAILED to false,
            ),
            emptyByReadiness,
        )
    }
}
