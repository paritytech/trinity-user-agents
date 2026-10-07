package io.paritytech.polkadotapp.feature_wallet_impl.presentation.enterAmount.compose.components

import org.junit.Assert.assertEquals
import org.junit.Test

class AmountSlotTest {
    @Test
    fun `amount that fits keeps its own width`() {
        val slot = amountSlot(boxWidth = 1000, widths = LockupWidths(fiatSymbol = 50f, amount = 300f, ticker = 150f))

        assertEquals(AmountSlot(width = 300f, overflows = false), slot)
    }

    @Test
    fun `amount wider than eighty percent is clamped to the remaining slot`() {
        val slot = amountSlot(boxWidth = 1000, widths = LockupWidths(fiatSymbol = 50f, amount = 900f, ticker = 150f))

        assertEquals(AmountSlot(width = 600f, overflows = true), slot)
    }

    @Test
    fun `amount exactly at the limit does not overflow`() {
        val slot = amountSlot(boxWidth = 1000, widths = LockupWidths(fiatSymbol = 50f, amount = 600f, ticker = 150f))

        assertEquals(AmountSlot(width = 600f, overflows = false), slot)
    }

    @Test
    fun `eighty percent limit rounds to the nearest pixel`() {
        val slot = amountSlot(boxWidth = 936, widths = LockupWidths(fiatSymbol = 50f, amount = 549f, ticker = 150f))

        assertEquals(false, slot.overflows)
        assertEquals(549f, slot.width)
    }

    @Test
    fun `symbol and ticker wider than the limit give a zero slot, not a negative one`() {
        val slot = amountSlot(boxWidth = 100, widths = LockupWidths(fiatSymbol = 50f, amount = 10f, ticker = 150f))

        assertEquals(AmountSlot(width = 0f, overflows = true), slot)
    }

    @Test
    fun `zero box gives zero slot`() {
        val slot = amountSlot(boxWidth = 0, widths = LockupWidths(fiatSymbol = 0f, amount = 0f, ticker = 0f))

        assertEquals(AmountSlot(width = 0f, overflows = false), slot)
    }
}
