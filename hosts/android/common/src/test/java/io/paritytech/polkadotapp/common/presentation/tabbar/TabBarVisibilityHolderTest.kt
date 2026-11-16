package io.paritytech.polkadotapp.common.presentation.tabbar

import org.junit.Assert.assertEquals
import org.junit.Test

class TabBarVisibilityHolderTest {
    private val holder = TabBarVisibilityHolder()

    @Test
    fun `a product pins the bar over itself`() {
        holder.pin(ProductA, TabBarPin.Tucked)

        assertEquals(TabBarMode.TuckedOverScreen(ProductA), holder.mode.value)
    }

    @Test
    fun `Main pins the bar without a product`() {
        holder.pin(Main, TabBarPin.Main)

        assertEquals(TabBarMode.Main, holder.mode.value)
    }

    @Test
    fun `a product opening while Main is still pinned is already over a product`() {
        holder.pin(Main, TabBarPin.Main)
        holder.pin(ProductA, TabBarPin.Tucked)

        assertEquals(TabBarMode.TuckedOverScreen(ProductA), holder.mode.value)
    }

    @Test
    fun `Main resuming before the product lets go is still over the product until it does`() {
        holder.pin(ProductA, TabBarPin.Tucked)
        holder.pin(Main, TabBarPin.Main)

        assertEquals(TabBarMode.TuckedOverScreen(ProductA), holder.mode.value)

        holder.release(ProductA)

        assertEquals(TabBarMode.Main, holder.mode.value)
    }

    @Test
    fun `a second product opened over the first is a new pin`() {
        holder.pin(ProductA, TabBarPin.Tucked)
        holder.pin(ProductB, TabBarPin.Tucked)

        assertEquals(TabBarMode.TuckedOverScreen(ProductB), holder.mode.value)

        holder.release(ProductB)

        assertEquals(TabBarMode.TuckedOverScreen(ProductA), holder.mode.value)
    }

    @Test
    fun `the bar leaves with the last screen that pinned it`() {
        holder.pin(ProductA, TabBarPin.Tucked)
        holder.release(ProductA)

        assertEquals(TabBarMode.Absent, holder.mode.value)
    }

    @Test
    fun `a chat keeps the bar away`() {
        holder.pin(Chat, TabBarPin.Hidden)

        assertEquals(TabBarMode.HiddenByScreen(Chat), holder.mode.value)
    }

    @Test
    fun `a chat opening while Main is still pinned already takes the bar away`() {
        holder.pin(Main, TabBarPin.Main)
        holder.pin(Chat, TabBarPin.Hidden)

        assertEquals(TabBarMode.HiddenByScreen(Chat), holder.mode.value)
    }

    @Test
    fun `a chat opening over a product takes the bar away until the chat closes`() {
        holder.pin(ProductA, TabBarPin.Tucked)
        holder.pin(Chat, TabBarPin.Hidden)

        assertEquals(TabBarMode.HiddenByScreen(Chat), holder.mode.value)

        holder.release(Chat)

        assertEquals(TabBarMode.TuckedOverScreen(ProductA), holder.mode.value)
    }

    @Test
    fun `Main comes back with the bar once the chat lets go`() {
        holder.pin(Chat, TabBarPin.Hidden)
        holder.pin(Main, TabBarPin.Main)

        assertEquals(TabBarMode.HiddenByScreen(Chat), holder.mode.value)

        holder.release(Chat)

        assertEquals(TabBarMode.Main, holder.mode.value)
    }

    private companion object {
        val Main = Any()
        val ProductA = Any()
        val ProductB = Any()
        val Chat = Any()
    }
}
