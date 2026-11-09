package io.paritytech.polkadotapp.common.presentation.tabbar

import org.junit.Assert.assertEquals
import org.junit.Test

class TabBarVisibilityHolderTest {
    private val holder = TabBarVisibilityHolder()

    @Test
    fun `a product pins the bar over itself`() {
        holder.forceShow(ProductA)

        assertEquals(Visibility(hidden = false, forced = true, productPin = ProductA), holder.visibility())
    }

    @Test
    fun `Main pins the bar without a product`() {
        holder.forceShow(Main)

        assertEquals(Visibility(hidden = false, forced = true, productPin = null), holder.visibility())
    }

    @Test
    fun `a product opening while Main is still pinned is already over a product`() {
        holder.forceShow(Main)
        holder.forceShow(ProductA)

        assertEquals(Visibility(hidden = false, forced = true, productPin = ProductA), holder.visibility())
    }

    @Test
    fun `Main resuming before the product lets go is still over the product until it does`() {
        holder.forceShow(ProductA)
        holder.forceShow(Main)

        assertEquals(Visibility(hidden = false, forced = true, productPin = ProductA), holder.visibility())

        holder.releaseForce(ProductA)

        assertEquals(Visibility(hidden = false, forced = true, productPin = null), holder.visibility())
    }

    @Test
    fun `a second product opened over the first is a new pin`() {
        holder.forceShow(ProductA)
        holder.forceShow(ProductB)

        assertEquals(Visibility(hidden = false, forced = true, productPin = ProductB), holder.visibility())

        holder.releaseForce(ProductB)

        assertEquals(Visibility(hidden = false, forced = true, productPin = ProductA), holder.visibility())
    }

    @Test
    fun `the bar leaves with the last screen that pinned it`() {
        holder.forceShow(ProductA)
        holder.releaseForce(ProductA)

        assertEquals(Visibility(hidden = true, forced = false, productPin = null), holder.visibility())
    }

    private data class Visibility(val hidden: Boolean, val forced: Boolean, val productPin: TabBarProductPin?)

    private fun TabBarVisibilityHolder.visibility() = Visibility(hidden.value, forced.value, productPin.value)

    private companion object {
        val Main = Any()
        val ProductA = TabBarProductPin()
        val ProductB = TabBarProductPin()
    }
}
