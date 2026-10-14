package io.paritytech.polkadotapp.feature_coinage_impl.domain.model

import io.novasama.substrate_sdk_android.extensions.toHexString
import io.paritytech.polkadotapp.chains.util.scaleEncodeSerializable
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.Coin
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.CoinProvenance
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.ValueExponent
import io.paritytech.polkadotapp.feature_coinage_impl.testKey
import org.junit.Assert.assertEquals
import org.junit.Test
import java.math.BigInteger

class SplitDestinationTest {
    @Test
    fun `coins of the same denomination share one destination`() {
        val destinations = listOf(
            coin(index = 0, exponent = -2),
            coin(index = 1, exponent = 3),
            coin(index = 2, exponent = -2),
        ).toSplitDestinations()

        assertEquals(
            listOf(
                SplitDestination(ValueExponent(-2), listOf(accountId(0), accountId(2))),
                SplitDestination(ValueExponent(3), listOf(accountId(1))),
            ),
            destinations
        )
    }

    /**
     * pallet-coinage v0.11.2 declares `split_into` as `Vec<(Denomination, Vec<AccountId>)>` for both
     * `split` and `unload_recycler_into_coins`; a flat list of account ids is rejected at encode time.
     */
    @Test
    fun `split destinations encode as denomination and account id list pairs`() {
        val encoded = listOf(coin(index = 0, exponent = -2), coin(index = 1, exponent = -2))
            .toSplitDestinations()
            .scaleEncodeSerializable()

        assertEquals(
            listOf(
                listOf(BigInteger.valueOf(-2), listOf(accountId(0).value.toHexString(), accountId(1).value.toHexString()))
            ),
            encoded.normalized()
        )
    }

    private fun coin(index: Int, exponent: Int) = Coin(
        derivationIndex = testKey(index),
        valueExponent = ValueExponent(exponent),
        age = Coin.Age.Known(0),
        isOnChain = true,
        accountId = accountId(index),
        provenance = CoinProvenance.UNKNOWN,
    )

    private fun accountId(index: Int): AccountId = ByteArray(32) { index.toByte() }.toDataByteArray()

    private fun Any?.normalized(): Any? = when (this) {
        is ByteArray -> toHexString()
        is List<*> -> map { it.normalized() }
        else -> this
    }
}
