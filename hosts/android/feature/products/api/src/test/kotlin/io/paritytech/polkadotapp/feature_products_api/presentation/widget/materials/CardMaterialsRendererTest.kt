package io.paritytech.polkadotapp.feature_products_api.presentation.widget.materials

import androidx.compose.runtime.mutableStateOf
import io.mockk.mockk
import org.junit.Assert.assertEquals
import org.junit.Test

class CardMaterialsRendererTest {
    private val card = mockk<CardMaterials>()

    private fun copy() = CardMaterialsRenderer(card, mutableStateOf(null), mutableStateOf(null))

    // The list card and the opened card are two copies of one card. Each hides a node's children only
    // once its own surface drew, or the opened copy would show a blank node until its first frame.
    @Test
    fun `each copy of a card waits for its own surface`() {
        val list = copy()
        val opened = copy()

        list.shown(0)
        val afterListDrew = listOf(list.showsSurface(0), opened.showsSurface(0))
        opened.shown(0)
        list.hidden(0)

        assertEquals(listOf(true, false), afterListDrew)
        assertEquals(listOf(false, true), listOf(list.showsSurface(0), opened.showsSurface(0)))
    }
}
