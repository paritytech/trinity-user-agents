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
        val pastTheOffsetWithAShortRun = holder.state.value

        // When
        holder.onScroll(downwardDp = 1f, atTop = false)

        // Then
        assertEquals(StatusBarScrollState(idBarHidden = false, contentUnderBar = true), pastTheOffsetWithAShortRun)
        assertEquals(StatusBarScrollState(idBarHidden = true, contentUnderBar = true), holder.state.value)
    }

    @Test
    fun `a 10dp scroll up brings the bar back and the next 16dp run hides it again`() {
        // Given
        holder.onScroll(downwardDp = 100f, atTop = false)
        holder.onScroll(downwardDp = -9f, atTop = false)
        val beforeTheThreshold = holder.state.value

        // When
        holder.onScroll(downwardDp = -1f, atTop = false)
        val backAfterTheScrollUp = holder.state.value
        holder.onScroll(downwardDp = 16f, atTop = false)

        // Then
        assertEquals(StatusBarScrollState(idBarHidden = true, contentUnderBar = true), beforeTheThreshold)
        assertEquals(StatusBarScrollState(idBarHidden = false, contentUnderBar = true), backAfterTheScrollUp)
        assertEquals(StatusBarScrollState(idBarHidden = true, contentUnderBar = true), holder.state.value)
    }

    @Test
    fun `content counts as under the bar only past the bar's height and stays so until the top`() {
        // Given
        holder.onScroll(downwardDp = 15f, atTop = false)
        val withinTheBarsHeight = holder.state.value

        // When
        holder.onScroll(downwardDp = 10f, atTop = false)
        holder.onScroll(downwardDp = -10f, atTop = false)

        // Then
        assertEquals(StatusBarScrollState.AtRest, withinTheBarsHeight)
        assertEquals(StatusBarScrollState(idBarHidden = false, contentUnderBar = true), holder.state.value)
    }

    @Test
    fun `reaching the top shows the bar without a plate`() {
        // Given
        holder.onScroll(downwardDp = 100f, atTop = false)

        // When
        holder.onScroll(downwardDp = -100f, atTop = true)

        // Then
        assertEquals(StatusBarScrollState.AtRest, holder.state.value)
    }
}
