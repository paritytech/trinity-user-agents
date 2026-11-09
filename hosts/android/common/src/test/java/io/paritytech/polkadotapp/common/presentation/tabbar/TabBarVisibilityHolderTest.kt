package io.paritytech.polkadotapp.common.presentation.tabbar

import org.junit.Assert.assertEquals
import org.junit.Test

class TabBarVisibilityHolderTest {
    private val holder = TabBarVisibilityHolder()

    @Test
    fun `a product pins the bar over itself`() {
        holder.pin(ProductA)

        assertEquals(Visibility(hidden = false, forced = true, tuckedPin = ProductA), holder.visibility())
    }

    @Test
    fun `Main pins the bar without a product`() {
        holder.pin(Main)

        assertEquals(Visibility(hidden = false, forced = true, tuckedPin = null), holder.visibility())
    }

    @Test
    fun `a product opening while Main is still pinned is already over a product`() {
        holder.pin(Main)
        holder.pin(ProductA)

        assertEquals(Visibility(hidden = false, forced = true, tuckedPin = ProductA), holder.visibility())
    }

    @Test
    fun `Main resuming before the product lets go is still over the product until it does`() {
        holder.pin(ProductA)
        holder.pin(Main)

        assertEquals(Visibility(hidden = false, forced = true, tuckedPin = ProductA), holder.visibility())

        holder.release(ProductA)

        assertEquals(Visibility(hidden = false, forced = true, tuckedPin = null), holder.visibility())
    }

    @Test
    fun `a second product opened over the first is a new pin`() {
        holder.pin(ProductA)
        holder.pin(ProductB)

        assertEquals(Visibility(hidden = false, forced = true, tuckedPin = ProductB), holder.visibility())

        holder.release(ProductB)

        assertEquals(Visibility(hidden = false, forced = true, tuckedPin = ProductA), holder.visibility())
    }

    @Test
    fun `the bar leaves with the last screen that pinned it`() {
        holder.pin(ProductA)
        holder.release(ProductA)

        assertEquals(Visibility(hidden = true, forced = false, tuckedPin = null), holder.visibility())
    }

    @Test
    fun `a chat keeps the bar away and says so`() {
        holder.pin(Chat)

        assertEquals(Visibility(hidden = true, forced = false, tuckedPin = null), holder.visibility())
        assertEquals(Chat, holder.hiddenPin.value)
    }

    @Test
    fun `leaving a chat for Main clears the hidden pin`() {
        holder.pin(Chat)
        holder.pin(Main)
        holder.release(Chat)

        assertEquals(Visibility(hidden = false, forced = true, tuckedPin = null), holder.visibility())
        assertEquals(null, holder.hiddenPin.value)
    }

    private data class Visibility(val hidden: Boolean, val forced: Boolean, val tuckedPin: TabBarTuckedPin?)

    private fun TabBarVisibilityHolder.visibility() = Visibility(hidden.value, forced.value, tuckedPin.value)

    private companion object {
        val Main = Any()
        val ProductA = TabBarTuckedPin()
        val ProductB = TabBarTuckedPin()
        val Chat = TabBarHiddenPin()
    }
}
