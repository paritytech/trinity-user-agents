package io.paritytech.polkadotapp.feature_products_impl.domain.browser

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class OpenTabNavigationTest {
    private val home = "https://meet.dot/"
    private val room = "https://meet.dot/room/?id=qof-qrb-gvk"

    @Test
    fun `loads the link into the open tab when it names a page`() {
        assertTrue(loadsIntoOpenTab(home, "$room&from=https%3A%2F%2Fcalendar.dot%2F&r=lq2x"))
    }

    @Test
    fun `loads the link into the open tab when it carries only a query`() {
        assertTrue(loadsIntoOpenTab(home, "https://meet.dot/?id=qof-qrb-gvk"))
    }

    @Test
    fun `loads the link into the open tab when it carries only a fragment`() {
        assertTrue(loadsIntoOpenTab(home, "https://meet.dot/#settings"))
    }

    @Test
    fun `only focuses the open tab when the link is the bare product`() {
        assertFalse(loadsIntoOpenTab(room, "https://meet.dot"))
        assertFalse(loadsIntoOpenTab(room, "https://meet.dot/"))
    }

    @Test
    fun `only focuses the open tab when it already shows the page`() {
        assertFalse(loadsIntoOpenTab(room, room))
    }

    @Test
    fun `only focuses the open tab when the link does not parse`() {
        assertFalse(loadsIntoOpenTab(home, "https://meet.dot/room/?id=a b"))
    }
}
