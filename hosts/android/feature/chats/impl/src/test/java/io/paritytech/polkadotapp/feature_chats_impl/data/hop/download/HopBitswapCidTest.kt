package io.paritytech.polkadotapp.feature_chats_impl.data.hop.download

import io.ipfs.multihash.Multihash
import io.paritytech.polkadotapp.tools_ipfs_api.Cid
import io.paritytech.polkadotapp.tools_ipfs_api.Cids
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class HopBitswapCidTest {
    @Test
    fun `cid is a base32 cidv1 raw blake2b256 over the entry hash`() {
        val hash = ByteArray(32) { it.toByte() }

        val cid = Cids.hopBitswapCid(hash)

        // multibase base32-lower is prefixed with 'b'
        assertTrue(cid.startsWith("b"))

        val decoded = Cid.decode(cid)
        assertEquals(1L, decoded.version)
        assertEquals(Cid.Codec.Raw, decoded.codec)
        assertEquals(Multihash.Type.blake2b_256, decoded.type)
        assertArrayEquals(hash, decoded.hash)
    }
}
