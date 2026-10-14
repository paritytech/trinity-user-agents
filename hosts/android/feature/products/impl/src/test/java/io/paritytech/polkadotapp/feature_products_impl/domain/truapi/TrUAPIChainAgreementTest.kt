package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.truapi.ChainIdentifier

/** RFC-0026 requires the advertised set and the dialable set to agree. */
class TrUAPIChainAgreementTest {
    private val relay = genesis(1)
    private val assetHub = genesis(2)

    @Test
    fun `a role with no endpoints is not advertised`() {
        val set = advertisedFrom(
            roles = listOf(ChainIdentifier.RELAY to relay, ChainIdentifier.ASSET_HUB to assetHub),
            endpoints = mapOf(relay.hexKey() to listOf("wss://relay.example")),
            network = "polkadot",
        )

        assertEquals(listOf(ChainIdentifier.RELAY), set.chains.map { it.identifier })
    }

    @Test
    fun `every advertised chain is dialable`() {
        val endpoints = mapOf(relay.hexKey() to listOf("wss://relay.example"))
        val chains = TrUAPIChains(
            advertised = advertisedFrom(
                roles = listOf(ChainIdentifier.RELAY to relay, ChainIdentifier.ASSET_HUB to assetHub),
                endpoints = endpoints,
                network = "polkadot",
            ),
            endpoints = endpoints,
        )

        assertTrue(chains.advertised.chains.all { chains.canDial(it.genesisHash) })
    }

    @Test
    fun `a chain with no endpoints is not dialable`() {
        val chains = TrUAPIChains(
            advertised = advertisedFrom(emptyList(), emptyMap(), network = "polkadot"),
            endpoints = mapOf(relay.hexKey() to emptyList()),
        )

        assertFalse(chains.canDial(relay.value))
        assertFalse(chains.canDial(assetHub.value))
    }

    @Test
    fun `an empty registry advertises nothing`() {
        val set = advertisedFrom(
            roles = listOf(ChainIdentifier.RELAY to relay),
            endpoints = emptyMap(),
            network = "polkadot",
        )

        assertTrue(set.chains.isEmpty())
    }

    private fun genesis(seed: Byte) = ByteArray(32) { seed }.toDataByteArray()
}
