package io.paritytech.polkadotapp.feature_account_impl.domain.derivation

import io.novasama.substrate_sdk_android.encrypt.junction.SubstrateJunctionDecoder
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_account_api.domain.derivation.DerivationIndex32
import io.paritytech.polkadotapp.feature_account_api.domain.derivation.asDisplayString
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class DerivationIndex32Test {
    @Test
    fun `index magic matches the RFC-0022 test vector`() {
        // blake2b256(utf8("product-account-index"))[..28] — generated once and pinned.
        val expected = "12e86013736c5498f050b03cdc16957dff0e422fb92ca77ec3ab168f".fromHexVector()

        assertArrayEquals(expected, DerivationIndex32.INDEX_MAGIC)
    }

    @Test
    fun `plain index is little-endian u32 followed by the magic`() {
        val index = DerivationIndex32.fromUInt(5u)
        val raw = index.bytes.value

        assertArrayEquals(byteArrayOf(5, 0, 0, 0), raw.copyOfRange(0, 4))
        assertArrayEquals(DerivationIndex32.INDEX_MAGIC, raw.copyOfRange(4, DerivationIndex32.SIZE_BYTES))
    }

    @Test
    fun `default is index zero`() {
        assertEquals(DerivationIndex32.fromUInt(0u), DerivationIndex32.default())
    }

    @Test
    fun `plain index round trips through the byte form`() {
        listOf(0u, 1u, 5u, UInt.MAX_VALUE).forEach { value ->
            assertEquals(value, DerivationIndex32.fromUInt(value).asUIntOrNull())
        }
    }

    @Test
    fun `raw selector without the magic is not a plain index`() {
        val raw = rawSelector()

        assertNull(raw.asUIntOrNull())
    }

    @Test
    fun `selector shorter than 32 bytes is rejected`() {
        val result = DerivationIndex32.fromBytes(byteArrayOf(1, 2, 3).toDataByteArray())

        assertTrue(result.isFailure)
    }

    /**
     * The whole "keep String derivation paths" design rests on this: the hex path segment has to
     * decode back to the exact 32 bytes, with no substrate string normalization in between.
     */
    @Test
    fun `path segment decodes back to the index bytes as a chain code`() {
        val index = DerivationIndex32.fromUInt(5u)

        val junctions = SubstrateJunctionDecoder.decode("//product//browse.dot/${index.asPathSegment()}").junctions

        assertArrayEquals(index.bytes.value, junctions.last().chaincode)
    }

    @Test
    fun `display string prefers the plain index`() {
        assertEquals("5", DerivationIndex32.fromUInt(5u).asDisplayString())

        val raw = rawSelector()
        assertEquals(raw.asPathSegment(), raw.asDisplayString())
    }

    private fun rawSelector(): DerivationIndex32 {
        return DerivationIndex32.fromBytes(ByteArray(DerivationIndex32.SIZE_BYTES) { 7 }.toDataByteArray()).getOrThrow()
    }
}

private fun String.fromHexVector(): ByteArray = chunked(2).map { it.toInt(16).toByte() }.toByteArray()
