package io.paritytech.polkadotapp.feature_chats_impl.presentation.list.compose

import org.junit.Assert.assertEquals
import org.junit.Test

class NewChatButtonOffsetYPxTest {
    @Test
    fun `the button stays above a shown Docklet`() {
        assertEquals(0, newChatButtonOffsetYPx(barInsetBottom = 300, navigationBarsBottom = 100, hiddenFraction = 0f))
    }

    @Test
    fun `a hidden Docklet lets the button drop to its own spacing above the navigation bar`() {
        assertEquals(200, newChatButtonOffsetYPx(barInsetBottom = 300, navigationBarsBottom = 100, hiddenFraction = 1f))
    }

    @Test
    fun `without a Docklet band the button does not move`() {
        assertEquals(0, newChatButtonOffsetYPx(barInsetBottom = 0, navigationBarsBottom = 100, hiddenFraction = 1f))
    }
}
