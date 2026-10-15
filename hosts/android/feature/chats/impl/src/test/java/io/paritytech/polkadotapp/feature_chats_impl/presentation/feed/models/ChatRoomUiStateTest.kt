package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.models

import io.paritytech.polkadotapp.feature_chats_impl.domain.roomPolicy.ChatRoomPolicy
import org.junit.Assert.assertEquals
import org.junit.Test

class ChatRoomUiStateTest {
    @Test
    fun `a person chat has no subtitle and no placeholder`() {
        assertEquals(ChatRoomUiState.PersonChat, null.toRoomUiState())
    }

    @Test
    fun `a product room without declarations shows the host placeholder`() {
        assertEquals(ChatRoomUiState(subtitle = null, emptyPlaceholder = EmptyRoomPlaceholder.HostText, confirmsLinks = true), ChatRoomPolicy.Defaults.toRoomUiState())
    }

    @Test
    fun `the description is the subtitle and the product placeholder replaces the host text`() {
        val policy = ChatRoomPolicy.Defaults.copy(description = "Forecasts for Berlin", placeholder = "Ask about the weather")

        assertEquals(
            ChatRoomUiState(subtitle = "Forecasts for Berlin", emptyPlaceholder = EmptyRoomPlaceholder.ProductText("Ask about the weather"), confirmsLinks = true),
            policy.toRoomUiState(),
        )
    }
}
