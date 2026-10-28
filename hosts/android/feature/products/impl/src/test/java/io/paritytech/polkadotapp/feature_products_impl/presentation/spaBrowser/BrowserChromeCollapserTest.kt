package io.paritytech.polkadotapp.feature_products_impl.presentation.spaBrowser

import org.junit.Assert.assertEquals
import org.junit.Test

class BrowserChromeCollapserTest {
    private val collapser = BrowserChromeCollapser(topPanelHeight = 100f)

    @Test
    fun `the panel follows the finger, hiding as the page scrolls down and returning as it scrolls up`() {
        // Given
        collapser.scrollTo(0f)

        // When
        val updates = listOf(250f, 200f).map { collapser.scrollTo(it) }

        // Then
        assertEquals(listOf(update(1f, animated = false), update(0.5f, animated = false)), updates)
    }

    @Test
    fun `a panel let go of halfway settles at the nearer end, so it is never left half hidden`() {
        // Given
        collapser.scrollTo(0f)
        collapser.scrollTo(60f)

        // When
        val update = collapser.restAt(60f)

        // Then
        assertEquals(update(1f, animated = true), update)
    }

    @Test
    fun `back at the top of the page the panel is fully shown, even after it settled hidden`() {
        // Given
        collapser.scrollTo(0f)
        collapser.scrollTo(60f)
        collapser.restAt(60f)

        // When
        val update = collapser.scrollTo(0f)

        // Then
        assertEquals(update(0f, animated = false), update)
    }

    private fun BrowserChromeCollapser.scrollTo(offset: Float) =
        update(BrowserChromeScrollSample(offsetToTopEdge = offset, isInteracting = true))

    private fun BrowserChromeCollapser.restAt(offset: Float) =
        update(BrowserChromeScrollSample(offsetToTopEdge = offset, isInteracting = false))

    private fun update(fraction: Float, animated: Boolean) = BrowserChromeCollapseUpdate(fraction, animated)
}
