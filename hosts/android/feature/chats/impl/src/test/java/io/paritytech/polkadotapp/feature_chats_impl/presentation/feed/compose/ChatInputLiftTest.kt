package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose

import org.junit.Assert.assertEquals
import org.junit.Test

class ChatInputLiftTest {
    @Test
    fun `a shown Docklet lifts the input onto its top edge`() {
        assertEquals(200, chatInputLiftPx(barInsetBottom = 300, systemBottom = 100, hiddenFraction = 0f))
    }

    @Test
    fun `a hidden Docklet lets the input take its place`() {
        assertEquals(0, chatInputLiftPx(barInsetBottom = 300, systemBottom = 100, hiddenFraction = 1f))
    }

    @Test
    fun `a keyboard taller than the Docklet band keeps the input on the keyboard`() {
        assertEquals(0, chatInputLiftPx(barInsetBottom = 300, systemBottom = 900, hiddenFraction = 0f))
    }
}
