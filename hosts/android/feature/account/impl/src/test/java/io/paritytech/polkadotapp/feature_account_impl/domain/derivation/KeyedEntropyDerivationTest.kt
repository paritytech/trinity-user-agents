package io.paritytech.polkadotapp.feature_account_impl.domain.derivation

import io.novasama.substrate_sdk_android.encrypt.junction.SubstrateJunctionDecoder
import io.paritytech.polkadotapp.common.utils.blake2b256
import io.paritytech.polkadotapp.feature_account_api.domain.derivation.deriveKeyedEntropy
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test

private val ACCOUNT_ENTROPY = ByteArray(32) { it.toByte() }

class KeyedEntropyDerivationTest {
    @Test
    fun `fold applies hash(parent, chainCode) per hard junction`() {
        val path = "//first//second"
        val codes = SubstrateJunctionDecoder.decode(path).junctions.map { it.chaincode }

        val expected = ACCOUNT_ENTROPY
            .blake2b256(key = codes[0])
            .blake2b256(key = codes[1])

        assertArrayEquals(expected, deriveKeyedEntropy(ACCOUNT_ENTROPY, path))
    }

    @Test(expected = IllegalArgumentException::class)
    fun `soft junction anywhere in the path is rejected`() {
        deriveKeyedEntropy(ACCOUNT_ENTROPY, "//domain/0")
    }

    @Test
    fun `ring-vrf and ecdh trees are rooted differently`() {
        val ringVrfRoot = ACCOUNT_ENTROPY.blake2b256(key = "ring-vrf".encodeToByteArray())
        val ecdhRoot = ACCOUNT_ENTROPY.blake2b256(key = "ecdh".encodeToByteArray())

        assertFalse(ringVrfRoot.contentEquals(ecdhRoot))
    }

    @Test
    fun `sibling paths under one root stay separated`() {
        val full = deriveKeyedEntropy(ACCOUNT_ENTROPY, "//peopl.dot//0x00")
        val light = deriveKeyedEntropy(ACCOUNT_ENTROPY, "//peopl.dot//0x01")

        assertEquals(32, full.size)
        assertFalse(full.contentEquals(light))
    }

    @Test
    fun `ecdh domains are separated`() {
        val chat = deriveKeyedEntropy(ACCOUNT_ENTROPY, "//chat")
        val sso = deriveKeyedEntropy(ACCOUNT_ENTROPY, "//sso")
        val game = deriveKeyedEntropy(ACCOUNT_ENTROPY, "//game")

        assertEquals(3, setOf(chat.toList(), sso.toList(), game.toList()).size)
    }
}
