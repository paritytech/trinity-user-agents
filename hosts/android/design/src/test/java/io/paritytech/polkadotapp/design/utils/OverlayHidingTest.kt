package io.paritytech.polkadotapp.design.utils

import android.view.View
import android.view.Window
import io.paritytech.polkadotapp.design.R
import io.paritytech.polkadotapp.test_shared.any
import io.paritytech.polkadotapp.test_shared.eq
import io.paritytech.polkadotapp.test_shared.whenever
import org.junit.Before
import org.junit.Test
import org.mockito.Mockito.doAnswer
import org.mockito.Mockito.mock
import org.mockito.Mockito.times
import org.mockito.Mockito.verify

class OverlayHidingTest {
    private val window: Window = mock()
    private val decorView: View = mock()
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
        whenever(window.decorView).thenReturn(decorView)
        whenever(decorView.getTag(R.id.overlay_hiding_count)).thenAnswer { countTag }
        doAnswer { invocation ->
            countTag = invocation.getArgument(1)
            null
        }.`when`(decorView).setTag(eq(R.id.overlay_hiding_count), any())
    }

    private fun verifyOverlaysHidden(count: Int) {
        verify(window, times(count)).setHideOverlayWindows(true)
    }

    private fun verifyOverlaysShown(count: Int) {
        verify(window, times(count)).setHideOverlayWindows(false)
    }
}
