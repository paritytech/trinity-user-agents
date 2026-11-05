package io.paritytech.polkadotapp.design.utils

import android.view.View
import android.view.Window
import io.mockk.Runs
import io.mockk.every
import io.mockk.just
import io.mockk.mockk
import io.mockk.verify
import io.paritytech.polkadotapp.design.R
import org.junit.Before
import org.junit.Test

class OverlayHidingTest {
    private val window: Window = mockk()
    private val decorView: View = mockk()
    private var countTag: Any? = null

    @Before
    fun setUp() {
        withDecorViewKeepingTags()
    }

    @Test
    fun `hides overlays when the first screen on a window is added`() {
        window.retainOverlayHiding()

        verifyOverlaysHidden(count = 1)
    }

    @Test
    fun `does not hide overlays again when a second screen is added`() {
        window.retainOverlayHiding()
        window.retainOverlayHiding()

        verifyOverlaysHidden(count = 1)
    }

    @Test
    fun `keeps overlays hidden when one of two screens is removed`() {
        window.retainOverlayHiding()
        window.retainOverlayHiding()
        window.releaseOverlayHiding()

        verifyOverlaysShown(count = 0)
    }

    @Test
    fun `shows overlays when the last of two screens is removed`() {
        window.retainOverlayHiding()
        window.retainOverlayHiding()
        window.releaseOverlayHiding()
        window.releaseOverlayHiding()

        verifyOverlaysShown(count = 1)
    }

    @Test
    fun `hides overlays again when a screen is added after the last one was removed`() {
        window.retainOverlayHiding()
        window.releaseOverlayHiding()
        window.retainOverlayHiding()

        verifyOverlaysHidden(count = 2)
    }

    private fun withDecorViewKeepingTags() {
        every { window.decorView } returns decorView
        every { window.setHideOverlayWindows(any()) } just Runs
        every { decorView.getTag(R.id.overlay_hiding_count) } answers { countTag }
        every { decorView.setTag(R.id.overlay_hiding_count, any()) } answers { countTag = secondArg() }
    }

    private fun verifyOverlaysHidden(count: Int) {
        verify(exactly = count) { window.setHideOverlayWindows(true) }
    }

    private fun verifyOverlaysShown(count: Int) {
        verify(exactly = count) { window.setHideOverlayWindows(false) }
    }
}
