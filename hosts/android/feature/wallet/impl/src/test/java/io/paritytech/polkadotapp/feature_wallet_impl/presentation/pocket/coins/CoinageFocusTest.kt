package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.coins

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class CoinageFocusTest {
    @Test
    fun `a band that fits below the coins goes there`() {
        val focus = focus(visibleTop = 0f, visibleBottom = 800f)

        assertEquals(300f, CoinageArrangement.bandTop(focus, coinsHeight = 300f), 0f)
        assertEquals(300f + BAND, CoinageArrangement.fieldHeight(focus, 300f, 300f), 0f)
    }

    @Test
    fun `a band that does not fit floats at the foot of the visible area`() {
        val focus = focus(visibleTop = 1000f, visibleBottom = 1600f)
        val top = CoinageArrangement.bandTop(focus, coinsHeight = 3000f)

        assertEquals(1600f - CoinageArrangement.FOCUS_INSET - BAND, top, 0f)
        assertEquals("a floating band does not grow the field", 3000f, CoinageArrangement.fieldHeight(focus, 3000f, top), 0f)
    }

    @Test
    fun `a floating band never rises above the visible top`() {
        val focus = focus(visibleTop = 500f, visibleBottom = 560f)

        assertEquals(500f, CoinageArrangement.bandTop(focus, coinsHeight = 3000f), 0f)
    }

    @Test
    fun `grades double in width and unknown is only for no score`() {
        val grades = listOf(null, 0, 8, 9, 16, 17, 32, 33, 64, 65, 100).map(CoinageCoinTrace.Quality::of)

        assertEquals(
            listOf(
                CoinageCoinTrace.Quality.UNKNOWN,
                CoinageCoinTrace.Quality.VERY_LOW,
                CoinageCoinTrace.Quality.VERY_LOW,
                CoinageCoinTrace.Quality.LOW,
                CoinageCoinTrace.Quality.LOW,
                CoinageCoinTrace.Quality.FAIR,
                CoinageCoinTrace.Quality.FAIR,
                CoinageCoinTrace.Quality.GOOD,
                CoinageCoinTrace.Quality.GOOD,
                CoinageCoinTrace.Quality.HIGH,
                CoinageCoinTrace.Quality.HIGH
            ),
            grades
        )
    }

    @Test
    fun `a tap finds the coin under it, the frontmost winning, and an edge-on coin keeps a floor`() {
        val field = CoinageCoinField()

        field.retarget(
            listOf(
                coin("back") to target(centreX = 50f, lift = 0f, turn = 0f),
                coin("front") to target(centreX = 60f, lift = 40f, turn = 0f),
                coin("edge") to target(centreX = 200f, lift = 0f, turn = (Math.PI / 2).toFloat())
            ),
            spawnEdge = 0f,
            stagger = CoinageCoinField.Stagger.FROM_FRONT
        )
        repeat(600) { field.advance(1f / 60) }

        assertEquals("front", CoinageScene.coinAt(55f, 30f, field))
        assertEquals("edge", CoinageScene.coinAt(206f, 30f, field))
        assertNull(CoinageScene.coinAt(215f, 30f, field))
    }

    private fun focus(visibleTop: Float, visibleBottom: Float) = CoinageArrangement.Focus(
        id = "picked",
        band = BAND,
        visibleTop = visibleTop,
        visibleBottom = visibleBottom
    )

    private fun coin(id: String) = CoinageScene.Coin(
        id = id,
        exponent = 0,
        wear = 0f,
        partition = CoinageStripLayout.Partition.READY,
        level = CoinageWear.maximumLevel,
        hops = 0
    )

    private fun target(centreX: Float, lift: Float, turn: Float) = CoinageCoinField.Target(
        centreX = centreX,
        centreY = 30f,
        height = 40f,
        turn = turn,
        thickness = 1f,
        wear = 0f,
        luster = 0f,
        calm = 0f,
        lift = lift
    )

    private companion object {
        const val BAND = CoinageArrangement.FOCUS_BAND_MINIMUM
    }
}
