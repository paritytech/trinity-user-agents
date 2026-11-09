package io.paritytech.polkadotapp.common.presentation.tabbar

import org.junit.Assert.assertEquals
import org.junit.Test

class TabBarVisibilityHolderTest {
    private val holder = TabBarVisibilityHolder()

    @Test
    fun `a product pins the bar over itself`() {
        holder.forceShow(Product, overProduct = true)

        assertEquals(Visibility(hidden = false, forced = true, overProduct = true), holder.visibility())
    }

    @Test
    fun `Main pins the bar without a product`() {
        holder.forceShow(Main, overProduct = false)

        assertEquals(Visibility(hidden = false, forced = true, overProduct = false), holder.visibility())
    }

    @Test
    fun `a product opening while Main is still pinned already counts as over a product`() {
        holder.forceShow(Main, overProduct = false)
        holder.forceShow(Product, overProduct = true)

        assertEquals(Visibility(hidden = false, forced = true, overProduct = true), holder.visibility())
    }

    @Test
    fun `leaving the product for Main drops the product mode and keeps the bar`() {
        holder.forceShow(Product, overProduct = true)
        holder.forceShow(Main, overProduct = false)
        holder.releaseForce(Product)

        assertEquals(Visibility(hidden = false, forced = true, overProduct = false), holder.visibility())
    }

    @Test
    fun `the bar leaves with the last screen that pinned it`() {
        holder.forceShow(Product, overProduct = true)
        holder.releaseForce(Product)

        assertEquals(Visibility(hidden = true, forced = false, overProduct = false), holder.visibility())
    }

    private data class Visibility(val hidden: Boolean, val forced: Boolean, val overProduct: Boolean)

    private fun TabBarVisibilityHolder.visibility() = Visibility(hidden.value, forced.value, overProduct.value)

    private companion object {
        val Main = Any()
        val Product = Any()
    }
}
