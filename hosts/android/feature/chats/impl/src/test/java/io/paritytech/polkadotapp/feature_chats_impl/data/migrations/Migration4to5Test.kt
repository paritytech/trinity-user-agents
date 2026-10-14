package io.paritytech.polkadotapp.feature_chats_impl.data.migrations

import io.paritytech.polkadotapp.chains.network.binding.intoBalance
import io.paritytech.polkadotapp.feature_chats_impl.data.migrations.schemas.LegacyCoinagePaymentStatusLocal
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.scale.CoinagePaymentStatusLocal
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class Migration4to5Test {
    private val fallback = 1000000000000.toBigInteger().intoBalance()

    @Test
    fun `DETECTED maps to Detected carrying the fallback balance`() {
        val result = LegacyCoinagePaymentStatusLocal.DETECTED.toSealed(fallback)

        assertTrue(result is CoinagePaymentStatusLocal.Detected)
        assertEquals(fallback, (result as CoinagePaymentStatusLocal.Detected).detected)
    }

    @Test
    fun `TRANSFERRED maps to Transferred carrying the fallback balance`() {
        val result = LegacyCoinagePaymentStatusLocal.TRANSFERRED.toSealed(fallback)

        assertTrue(result is CoinagePaymentStatusLocal.Transferred)
        assertEquals(fallback, (result as CoinagePaymentStatusLocal.Transferred).transferred)
    }

    @Test
    fun `field-less statuses map to their sealed counterparts`() {
        assertEquals(CoinagePaymentStatusLocal.Detecting, LegacyCoinagePaymentStatusLocal.DETECTING.toSealed(fallback))
        assertEquals(CoinagePaymentStatusLocal.FailedDetection, LegacyCoinagePaymentStatusLocal.FAILED_DETECTION.toSealed(fallback))
        assertEquals(CoinagePaymentStatusLocal.FailedTransfer, LegacyCoinagePaymentStatusLocal.FAILED_TRANSFER.toSealed(fallback))
    }
}
