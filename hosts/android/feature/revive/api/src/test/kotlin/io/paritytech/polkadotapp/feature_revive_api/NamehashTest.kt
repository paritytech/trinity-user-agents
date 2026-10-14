package io.paritytech.polkadotapp.feature_revive_api

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Test

@OptIn(ExperimentalStdlibApi::class)
class NamehashTest {
    @Test
    fun `empty name returns 32 zero bytes`() {
        val result = NameHash.nameHash("")
        assertArrayEquals(ByteArray(32), result)
    }

    @Test
    fun `namehash of eth matches known ENS test vector`() {
        // Known ENS test vector: namehash("eth")
        val expected = "93cdeb708b7545dc668eb9280176169d1c33cfd8ed6f04690a0bcc88a93fc4ae"
        val result = NameHash.nameHash("eth")
        assertEquals(expected, result.toHexString())
    }

    @Test
    fun `namehash of foo_eth matches known ENS test vector`() {
        // Known ENS test vector: namehash("foo.eth")
        val expected = "de9b09fd7c5f901e23a3f19fecc54828e9c848539801e86591bd9801b019f84f"
        val result = NameHash.nameHash("foo.eth")
        assertEquals(expected, result.toHexString())
    }

    @Test
    fun `namehash of alice_eth matches known ENS test vector`() {
        // Known ENS test vector: namehash("alice.eth")
        val expected = "787192fc5378cc32aa956ddfdedbf26b24e8d78e40109add0eea2c1a012c3dec"
        val result = NameHash.nameHash("alice.eth")
        assertEquals(expected, result.toHexString())
    }

    @Test
    fun `namehash of dot domain`() {
        // namehash("product.dot") should produce a 32-byte result
        val result = NameHash.nameHash("product.dot")
        assertEquals(32, result.size)
    }

    @Test
    fun `node under tld matches namehash for a label without dots`() {
        assertArrayEquals(NameHash.nameHash("alice.dot"), NameHash.nodeUnderTld("dot", "alice"))
    }

    @Test
    fun `node of a lite label hashes the dotted label whole`() {
        val liteNode = NameHash.nodeUnderTld("dot", "alice.42")

        assert(!liteNode.contentEquals(NameHash.nameHash("alice.42.dot"))) {
            "A lite label is one labelhash, not a subdomain of the stem"
        }
        assertEquals(32, liteNode.size)
    }

    @Test
    fun `namehash is deterministic`() {
        val result1 = NameHash.nameHash("test.dot")
        val result2 = NameHash.nameHash("test.dot")
        assertArrayEquals(result1, result2)
    }

    @Test
    fun `different names produce different hashes`() {
        val result1 = NameHash.nameHash("alice.dot")
        val result2 = NameHash.nameHash("bob.dot")
        assert(!result1.contentEquals(result2)) { "Different names should produce different hashes" }
    }
}
