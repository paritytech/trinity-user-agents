package io.paritytech.polkadotapp.feature_products_impl.presentation.bot.footer.web

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

private const val PRODUCT = "dim2.dot"

class ProductRoomFooterWebPageTest {
    @Test
    fun `a relative path lands on the product origin with its query kept`() {
        assertEquals(
            "https://dim2.dot/slide-footer.html?label=Slide%20to%20register&action=jollity.register&overlay=1",
            ProductRoomFooterWebPage.url("slide-footer.html?label=Slide%20to%20register&action=jollity.register&overlay=1", PRODUCT),
        )
        assertEquals("https://dim2.dot/footers/slide.html#top", ProductRoomFooterWebPage.url("footers/slide.html#top", PRODUCT))
        assertEquals("https://dim2.dot/slide.html", ProductRoomFooterWebPage.url("  slide.html \n", PRODUCT))
    }

    /** Whatever the core let through, the host never loads a page from anywhere but the product's own origin. */
    @Test
    fun `a path that would leave the product origin is refused`() {
        val refused = listOf(
            "",
            "   ",
            "/slide.html",
            "//evil.example/slide.html",
            "https://evil.example/slide.html",
            "http://dim2.dot/slide.html",
            "javascript:alert(1)",
            "data:text/html,hi",
            "file:///etc/hosts",
            "..\\slide.html",
            "a\\b.html",
            "../slide.html",
            "a/../../slide.html",
            "%2e%2e/slide.html",
            "a/%2E%2E/slide.html",
            "https://user@dim2.dot/slide.html",
            "https://dim2.dot:8443/slide.html",
            "slide footer.html",
        )

        assertEquals(refused.map { null }, refused.map { ProductRoomFooterWebPage.url(it, PRODUCT) })
    }

    @Test
    fun `the product origin is matched on scheme and host alone`() {
        assertTrue(ProductRoomFooterWebPage.isProductOrigin("https://dim2.dot/slide.html?x=1", PRODUCT))
        assertTrue(ProductRoomFooterWebPage.isProductOrigin("HTTPS://DIM2.DOT/", PRODUCT))
        assertFalse(ProductRoomFooterWebPage.isProductOrigin("https://dim2.dot.evil.example/", PRODUCT))
        assertFalse(ProductRoomFooterWebPage.isProductOrigin("https://other.dot/", PRODUCT))
        assertFalse(ProductRoomFooterWebPage.isProductOrigin("http://dim2.dot/", PRODUCT))
        assertFalse(ProductRoomFooterWebPage.isProductOrigin("https://x@dim2.dot/", PRODUCT))
        assertFalse(ProductRoomFooterWebPage.isProductOrigin("https://dim2.dot:444/", PRODUCT))
        assertFalse(ProductRoomFooterWebPage.isProductOrigin("not a url", PRODUCT))
    }

    @Test
    fun `navigation is allowed only on the product origin and about blank`() {
        assertTrue(ProductRoomFooterWebPage.isAllowedNavigation("https://dim2.dot/other.html", PRODUCT))
        assertTrue(ProductRoomFooterWebPage.isAllowedNavigation("about:blank", PRODUCT))
        assertFalse(ProductRoomFooterWebPage.isAllowedNavigation("about:srcdoc", PRODUCT))
        assertFalse(ProductRoomFooterWebPage.isAllowedNavigation("https://polkadot.network/", PRODUCT))
        assertFalse(ProductRoomFooterWebPage.isAllowedNavigation("intent://scan/#Intent;scheme=zxing;end", PRODUCT))
    }

    @Test
    fun `heights are capped to 44 to 160`() {
        assertEquals(listOf(44, 44, 104, 160), listOf(0, 10, 104, 5000).map(ProductRoomFooterWebPage::height))
        assertEquals(
            listOf(null, null, null, null, 44, 97, 160),
            listOf(null, 0.0, -3.0, Double.NaN, 12.0, 96.2, 900.0).map(ProductRoomFooterWebPage::height),
        )
    }
}

class ProductFooterMessageTest {
    @Test
    fun `the four kinds are read`() {
        assertEquals(
            listOf(
                ProductFooterMessage.Action("jollity.register", null),
                ProductFooterMessage.Haptic("success"),
                ProductFooterMessage.Haptic("success"),
                ProductFooterMessage.Overlay(true),
                ProductFooterMessage.Overlay(false),
                ProductFooterMessage.Ready(96.0),
                ProductFooterMessage.Ready(null),
            ),
            listOf(
                """{"kind":"action","actionId":"jollity.register"}""",
                """{"kind":"haptic","style":"success"}""",
                """{"kind":"haptic"}""",
                """{"kind":"overlay","on":true}""",
                """{"kind":"overlay","on":false}""",
                """{"kind":"ready","height":96}""",
                """{"kind":"ready"}""",
            ).map(ProductFooterMessage::parse),
        )
    }

    @Test
    fun `a text payload travels as it is and any other JSON as its text`() {
        assertEquals(
            listOf(
                ProductFooterMessage.Action("a", "hello"),
                ProductFooterMessage.Action("a", """{"n":1}"""),
                ProductFooterMessage.Action("a", "[1,2]"),
                ProductFooterMessage.Action("a", "7"),
                ProductFooterMessage.Action("a", null),
                ProductFooterMessage.Action("a", null),
            ),
            listOf(
                """{"kind":"action","actionId":"a","payload":"hello"}""",
                """{"kind":"action","actionId":"a","payload":{"n":1}}""",
                """{"kind":"action","actionId":"a","payload":[1,2]}""",
                """{"kind":"action","actionId":"a","payload":7}""",
                """{"kind":"action","actionId":"a","payload":null}""",
                """{"kind":"action","actionId":"a","payload":""}""",
            ).map(ProductFooterMessage::parse),
        )
    }

    @Test
    fun `unreadable, unknown and oversized messages are dropped`() {
        val dropped = listOf(
            "",
            "not json",
            "[]",
            "\"action\"",
            """{"kind":"navigate","url":"https://evil.example"}""",
            """{"actionId":"a"}""",
            """{"kind":"action"}""",
            """{"kind":"action","actionId":""}""",
            """{"kind":"action","actionId":7}""",
            """{"kind":"action","actionId":"${"a".repeat(257)}"}""",
            """{"kind":"action","actionId":"a","payload":"${"p".repeat(64 * 1024 + 1)}"}""",
            """{"kind":"overlay"}""",
            """{"kind":"overlay","on":"true"}""",
        )

        assertEquals(dropped.map { null }, dropped.map(ProductFooterMessage::parse))
        assertEquals(ProductFooterMessage.Ready(null), ProductFooterMessage.parse("""{"kind":"ready","height":"96"}"""))
    }

    @Test
    fun `the limits are inclusive`() {
        val id = "a".repeat(ProductFooterMessage.ACTION_ID_MAX_BYTES)
        val payload = "p".repeat(ProductFooterMessage.PAYLOAD_MAX_BYTES)

        assertEquals(
            ProductFooterMessage.Action(id, payload),
            ProductFooterMessage.parse("""{"kind":"action","actionId":"$id","payload":"$payload"}"""),
        )
    }
}

class ProductFooterHostEventTest {
    @Test
    fun `host events call the page receiver only when it defines one`() {
        assertEquals(
            "window.productFooter && typeof window.productFooter.receive === 'function' && " +
                "window.productFooter.receive({\"kind\":\"state\",\"state\":\"confirmed\"});",
            ProductFooterHostEvent.Confirmed.script,
        )
        assertEquals("""{"kind":"reset"}""", ProductFooterHostEvent.Reset.json)
    }

    @Test
    fun `a second press inside the window is dropped`() {
        var now = 1_000L
        val debounce = ProductFooterActionDebounce(500) { now }

        val accepted = mutableListOf(debounce.tryAccept())
        now += 499
        accepted += debounce.tryAccept()
        now += 1
        accepted += debounce.tryAccept()

        assertEquals(listOf(true, false, true), accepted)
    }
}
