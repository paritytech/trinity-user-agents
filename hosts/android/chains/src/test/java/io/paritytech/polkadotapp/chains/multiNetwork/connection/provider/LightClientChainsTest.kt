package io.paritytech.polkadotapp.chains.multiNetwork.connection.provider

import io.mockk.mockk
import io.parity.truapi.LightClient
import io.paritytech.polkadotapp.common.data.network.TestnetEnvironment
import org.junit.Assert.assertEquals
import org.junit.Test
import uniffi.truapi_provider.ChainCloseReason
import uniffi.truapi_provider.ChainMessageListener
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit

class LightClientChainsTest {
    // The light client reaches only the chains truapi-provider bundles. A genesis it does not know
    // would leave that chain unable to connect at all, so every listed one must answer its own
    // genesis hash, which the bundled spec does without any network.
    @Test
    fun `every chain of a build's network is one the light client can reach`() {
        val listed = TestnetEnvironment.entries.flatMap(LightClientChains::of)
        val lightClient = LightClient(storage = mockk(relaxed = true), chains = listed.toSet())

        val answered = listed.map { genesis -> genesisAnsweredBy(lightClient, genesis) }

        assertEquals(listed.map { "0x$it" }, answered)
    }

    private fun genesisAnsweredBy(lightClient: LightClient, genesis: String): String? {
        val answers = LinkedBlockingQueue<String>()
        val listener = object : ChainMessageListener {
            override fun onMessage(message: String) {
                answers.add(RESULT.find(message)?.groupValues?.get(1).orEmpty())
            }

            override fun onClosed(reason: ChainCloseReason) = Unit
        }
        lightClient.connect(genesis.hexToByteArray(), listener).use { connection ->
            connection.send(GENESIS_REQUEST)
            return answers.poll(30, TimeUnit.SECONDS)
        }
    }

    private companion object {
        const val GENESIS_REQUEST = """{"jsonrpc":"2.0","id":1,"method":"chainSpec_v1_genesisHash","params":[]}"""
        val RESULT = Regex(""""result":"(0x[0-9a-f]+)"""")
    }
}
