package io.paritytech.polkadotapp.feature_coinage_impl

import io.paritytech.polkadotapp.feature_coinage_api.domain.model.Hop
import io.paritytech.polkadotapp.feature_coinage_impl.data.mappers.decodeCoinHops
import io.paritytech.polkadotapp.feature_coinage_impl.data.mappers.encodeCoinHops
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Pins the on-disk encoding of `coins.hops`.
 *
 * These hex strings are what every already-stored row is written in, so they must never be edited. A change
 * that makes this test fail is a change that corrupts existing coins: freeze the old shape as
 * `CoinHopsLocalV60`, add a migration, and add a new test beside this one.
 */
class CoinHopsScaleConformanceTest {
    @Test
    fun `no hops encodes as an empty vector`() {
        assertEquals(EMPTY_HOPS, emptyList<Hop>().encodeCoinHops().toHex())
    }

    @Test
    fun `hops encode as a vector of indexed variants`() {
        assertEquals(MIXED_HOPS, mixedHops.encodeCoinHops().toHex())
    }

    @Test
    fun `stored hops decode back to what was written`() {
        assertEquals(mixedHops, MIXED_HOPS.fromHex().decodeCoinHops().getOrThrow())
        assertEquals(emptyList<Hop>(), EMPTY_HOPS.fromHex().decodeCoinHops().getOrThrow())
    }

    /** A column that predates the hops blob is "no hops" rather than a failure. */
    @Test
    fun `an absent blob decodes as no hops`() {
        assertEquals(emptyList<Hop>(), null.decodeCoinHops().getOrThrow())
        assertEquals(emptyList<Hop>(), byteArrayOf().decodeCoinHops().getOrThrow())
    }

    /** Corruption is reported rather than swallowed: what to do about it is the caller's decision. */
    @Test
    fun `an unreadable blob decodes as a failure`() {
        assertTrue(byteArrayOf(0x7f, 0x7f, 0x7f).decodeCoinHops().isFailure)
    }

    private val mixedHops = listOf(Hop.Transfer.of(bundleSize = 1), Hop.Split.of(fanout = 3), Hop.Transfer.of(bundleSize = 255))

    private fun ByteArray.toHex() = joinToString("") { "%02x".format(it) }

    private fun String.fromHex() = chunked(2).map { it.toInt(16).toByte() }.toByteArray()

    private companion object {
        const val EMPTY_HOPS = "00"
        const val MIXED_HOPS = "0c0001000000010300000000ff000000"
    }
}
