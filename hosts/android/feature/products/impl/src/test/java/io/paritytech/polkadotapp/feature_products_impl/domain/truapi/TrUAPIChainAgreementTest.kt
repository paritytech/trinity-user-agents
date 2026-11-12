package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** `featureSupported(Chain)` must not promise a chain `chainConnect` refuses to dial. */
class TrUAPIChainAgreementTest {
    private val relay = genesis(1)
    private val assetHub = genesis(2)

    @Test
    fun `a chain with endpoints is dialable`() {
        val chains = TrUAPIChains(endpoints = mapOf(relay.hexKey() to listOf("wss://relay.example")))

        assertTrue(chains.canDial(relay.value))
    }

    @Test
    fun `a chain with no endpoints is not dialable`() {
        val chains = TrUAPIChains(endpoints = mapOf(relay.hexKey() to emptyList()))

        assertFalse(chains.canDial(relay.value))
        assertFalse(chains.canDial(assetHub.value))
    }

    private fun genesis(seed: Byte) = ByteArray(32) { seed }.toDataByteArray()
}
