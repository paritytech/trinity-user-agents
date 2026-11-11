package io.paritytech.polkadotapp.common.presentation.statusBar

import org.junit.Assert.assertEquals
import org.junit.Test

class StatusBarScrollHolderTest {
    private val holder = StatusBarScrollHolder()

    @Test
    fun `the bar hides only after an unbroken 16dp downward run once the page is past 40dp`() {
        // Given
        holder.onScroll(downwardDp = 30f, atTop = false)
        holder.onScroll(downwardDp = -1f, atTop = false)
        holder.onScroll(downwardDp = 15f, atTop = false)
        val pastTheOffsetWithAShortRun = holder.state.idBarHidden

        // When
        holder.onScroll(downwardDp = 1f, atTop = false)

        // Then
        assertEquals(false, pastTheOffsetWithAShortRun)
        assertEquals(true, holder.state.idBarHidden)
    }

    @Test
    fun `a 10dp scroll up brings the bar back and the next 16dp run hides it again`() {
        // Given
        holder.onScroll(downwardDp = 100f, atTop = false)
        holder.onScroll(downwardDp = -9f, atTop = false)
        val beforeTheThreshold = holder.state.idBarHidden

        // When
        holder.onScroll(downwardDp = -1f, atTop = false)
        val backAfterTheScrollUp = holder.state.idBarHidden
        holder.onScroll(downwardDp = 16f, atTop = false)

        // Then
        assertEquals(true, beforeTheThreshold)
        assertEquals(false, backAfterTheScrollUp)
        assertEquals(true, holder.state.idBarHidden)
    }

    @Test
    fun `reaching the top shows the bar`() {
        // Given
        holder.onScroll(downwardDp = 100f, atTop = false)

        // When
        holder.onScroll(downwardDp = -100f, atTop = true)

        // Then
        assertEquals(false, holder.state.idBarHidden)
    }
}
