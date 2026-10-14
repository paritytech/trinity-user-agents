package io.paritytech.polkadotapp.bandersnatch_crypto

import android.util.Log
import org.junit.Test
import kotlin.time.measureTimedValue

class BandersnatchCryptoTest {

    private val context = BandersnatchContext.fromString("pop:polkadot.network/mob-rule   ")

    @Test
    fun shouldGeneratePublicKey() {
        val entropy = entropy(0)
        val publicKey = entropy.memberKey()
        Log.d("BandersnatchCryptoTest", publicKey.value.joinToString())
    }

    @OptIn(ExperimentalStdlibApi::class)
    @Test
    fun shouldGenerateProof() {
        val aliceEntropy = entropy(0)
        val alice = aliceEntropy.memberKey()

        val context = "pop:polkadot.network/mob-rule   ".encodeToByteArray()
        val message = "hello".encodeToByteArray()

        val allMembers = (1..100).map {
            entropy(it.toByte()).memberKey()
        } + alice

        val (result, time) = measureTimedValue {
            aliceEntropy.createProof(allMembers, context, message, BandersnatchDomainSize.Domain11)
        }
        Log.d("BandersnatchCryptoTest", "Generated proof in $time: ${result.proof.value.toHexString()}")
        Log.d("BandersnatchCryptoTest", "Proof alias: ${result.alias.value.toHexString()}")
    }

    @OptIn(ExperimentalStdlibApi::class)
    @Test
    fun showGenerateAliasInContext() {
        val entropy = entropy(0)
        val result = entropy.aliasInContext(context)

        Log.d("BandersnatchCryptoTest", "Generated alias ${result.value.toHexString()}")
    }

    private fun entropy(index: Byte): BandersnatchEntropy {
        val entropy = ByteArray(32) { index }
        return BandersnatchEntropy(entropy)
    }
}
