package io.paritytech.polkadotapp.feature_products_impl.domain.pocket

import org.junit.Assert.assertEquals
import org.junit.Test

class DebugServedUrlTest {
    /** The page tells which card it was opened for by this query, so a served page must receive it. */
    @Test
    fun `the card the page was opened for is passed on to the served page`() {
        assertEquals(
            "http://127.0.0.1:5173/?card=loyalty",
            debugServedUrl("http://127.0.0.1:5173/", "https://coinflip.dot?card=loyalty"),
        )
    }

    @Test
    fun `a query the served url already has is kept`() {
        assertEquals(
            "http://127.0.0.1:5173/app?debug=1&card=loyalty",
            debugServedUrl("http://127.0.0.1:5173/app?debug=1", "https://coinflip.dot/?card=loyalty"),
        )
    }

    @Test
    fun `a fragment of the served url stays after the query`() {
        assertEquals(
            "http://127.0.0.1:5173/?card=loyalty#/home",
            debugServedUrl("http://127.0.0.1:5173/#/home", "https://coinflip.dot?card=loyalty"),
        )
    }

    @Test
    fun `a launch without a query serves the page as entered`() {
        assertEquals(
            "http://127.0.0.1:5173/app?debug=1",
            debugServedUrl("http://127.0.0.1:5173/app?debug=1", "https://coinflip.dot"),
        )
    }

    @Test
    fun `an encoded query is passed on without being decoded`() {
        assertEquals(
            "http://127.0.0.1:5173/?card=a%26b",
            debugServedUrl("http://127.0.0.1:5173/", "https://coinflip.dot?card=a%26b"),
        )
    }
}
