package io.paritytech.polkadotapp.common.presentation.tabbar

import org.junit.Assert.assertEquals
import org.junit.Test

class TabBarVisibilityHolderTest {
    private val holder = TabBarVisibilityHolder()

    @Test
    fun `a product pins the bar over itself`() {
        holder.pin(ProductA)

        assertEquals(shown(tuckedPin = ProductA), holder.visibility())
    }

    @Test
    fun `Main pins the bar without a product`() {
        holder.pin(Main)

        assertEquals(shown(), holder.visibility())
    }

    @Test
    fun `a product opening while Main is still pinned is already over a product`() {
        holder.pin(Main)
        holder.pin(ProductA)

        assertEquals(shown(tuckedPin = ProductA), holder.visibility())
    }

    @Test
    fun `Main resuming before the product lets go is still over the product until it does`() {
        holder.pin(ProductA)
        holder.pin(Main)

        assertEquals(shown(tuckedPin = ProductA), holder.visibility())

        holder.release(ProductA)

        assertEquals(shown(), holder.visibility())
    }

    @Test
    fun `a second product opened over the first is a new pin`() {
        holder.pin(ProductA)
        holder.pin(ProductB)

        assertEquals(shown(tuckedPin = ProductB), holder.visibility())

        holder.release(ProductB)

        assertEquals(shown(tuckedPin = ProductA), holder.visibility())
    }

    @Test
    fun `the bar leaves with the last screen that pinned it`() {
        holder.pin(ProductA)
        holder.release(ProductA)

        assertEquals(absent(), holder.visibility())
    }

    @Test
    fun `a chat keeps the bar away`() {
        holder.pin(Chat)

        assertEquals(absent(hiddenPin = Chat), holder.visibility())
    }

    @Test
    fun `a chat opening while Main is still pinned already takes the bar away`() {
        holder.pin(Main)
        holder.pin(Chat)

        assertEquals(absent(hiddenPin = Chat), holder.visibility())
    }

    @Test
    fun `a chat opening over a product takes the bar away and keeps the product's pin`() {
        holder.pin(ProductA)
        holder.pin(Chat)

        assertEquals(absent(tuckedPin = ProductA, hiddenPin = Chat), holder.visibility())
    }

    @Test
    fun `Main comes back with the bar once the chat lets go`() {
        holder.pin(Chat)
        holder.pin(Main)

        assertEquals(absent(hiddenPin = Chat), holder.visibility())

        holder.release(Chat)

        assertEquals(shown(), holder.visibility())
    }

    private data class Visibility(
        val hidden: Boolean,
        val forced: Boolean,
        val tuckedPin: TabBarTuckedPin?,
        val hiddenPin: TabBarHiddenPin?,
    )

    private fun shown(tuckedPin: TabBarTuckedPin? = null) = Visibility(hidden = false, forced = true, tuckedPin = tuckedPin, hiddenPin = null)

    private fun absent(tuckedPin: TabBarTuckedPin? = null, hiddenPin: TabBarHiddenPin? = null) =
        Visibility(hidden = true, forced = false, tuckedPin = tuckedPin, hiddenPin = hiddenPin)

    private fun TabBarVisibilityHolder.visibility() = Visibility(hidden.value, forced.value, tuckedPin.value, hiddenPin.value)

    private companion object {
        val Main = Any()
        val ProductA = TabBarTuckedPin()
        val ProductB = TabBarTuckedPin()
        val Chat = TabBarHiddenPin()
    }
}
