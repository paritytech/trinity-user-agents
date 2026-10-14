package io.paritytech.polkadotapp.feature_xcm_impl.converter.chain

import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.GenesisHash
import io.paritytech.polkadotapp.chains.util.Geneses
import io.paritytech.polkadotapp.chains.util.Ids
import io.paritytech.polkadotapp.feature_xcm_api.config.model.ChainXcmConfig
import io.paritytech.polkadotapp.feature_xcm_api.multiLocation.AbsoluteMultiLocation
import io.paritytech.polkadotapp.feature_xcm_api.multiLocation.Junctions
import io.paritytech.polkadotapp.feature_xcm_api.multiLocation.MultiLocation.Interior
import io.paritytech.polkadotapp.feature_xcm_api.multiLocation.MultiLocation.Junction.GlobalConsensus
import io.paritytech.polkadotapp.feature_xcm_api.multiLocation.MultiLocation.Junction.ParachainId
import io.paritytech.polkadotapp.feature_xcm_api.multiLocation.RelativeMultiLocation
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Before
import org.junit.Test

class RealChainLocationConverterTest {
    private val xcmConfig: ChainXcmConfig = mockk()

    private val chainRegistry: ChainRegistry = mockk()

    private val polkadot: Chain = mockk()

    private val pah: Chain = mockk()

    private val kusama: Chain = mockk()

    private val kah: Chain = mockk()

    private lateinit var converter: RealChainLocationConverter

    private val polkadotId = Chain.Ids.POLKADOT
    private val polkadotGenesis = Chain.Geneses.POLKADOT

    private val pahId = Chain.Ids.PREVIEWNET_ASSET_HUB
    private val pahGenesis = Chain.Geneses.POLKADOT_ASSET_HUB

    private val kusamaId = Chain.Ids.KUSAMA
    private val kusamaGenesis = Chain.Geneses.KUSAMA

    private val kahId = Chain.Ids.KUSAMA_ASSET_HUB
    private val kahGenesis = Chain.Geneses.KUSAMA_ASSET_HUB

    private val paraId = 1000.toBigInteger()

    @Before
    fun setUp() = runBlocking {
        every { xcmConfig.parachainIds } returns mapOf(
            // In difference consensuses, same para id can be used
            pahId to paraId,
            kahId to paraId
        )

        setupRelay(polkadot, polkadotId, polkadotGenesis)
        setupChain(pah, pahId, pahGenesis, parentId = polkadotId)

        setupRelay(kusama, kusamaId, kusamaGenesis)
        setupChain(kah, kahId, kahGenesis, parentId = kusamaId)

        every { chainRegistry.chainsById } answers { flowOf(allChainsById()) }
        every { chainRegistry.currentChains } answers { flowOf(allChains()) }

        converter = RealChainLocationConverter(xcmConfig, chainRegistry)
    }

    private fun allChainsById(): Map<ChainId, Chain> {
        return allChains().associateBy { it.id }
    }

    private fun allChains(): List<Chain> {
        return listOf(polkadot, pah, kusama, kah)
    }

    private fun setupRelay(relaychain: Chain, chainId: String, genesisHash: GenesisHash) {
        setupChain(relaychain, chainId, genesisHash, parentId = null)
    }

    private fun setupChain(
        chain: Chain,
        chainId: String,
        genesisHash: GenesisHash,
        parentId: String?
    ) {
        every { chain.id } returns chainId
        every { chain.genesisHash } returns genesisHash
        every { chain.parentId } returns parentId
        coEvery { chainRegistry.getChain(chainId) } returns chain
    }

    @Test
    fun `chainFromRelativeLocation should return relaychain when location points to relaychain`() = runBlocking {
        val relativeLocation = RelativeMultiLocation(parents = 1, interior = Interior.Here)

        val result = converter.chainFromRelativeLocation(relativeLocation, pah)

        assertEquals(polkadot, result)
    }

    @Test
    fun `chainFromRelativeLocation should return parachain when location points to parachain`() = runBlocking {
        val relativeLocation = RelativeMultiLocation(parents = 0, interior = Junctions(ParachainId(paraId)))

        val result = converter.chainFromRelativeLocation(relativeLocation, polkadot)

        assertEquals(pah, result)
    }

    @Test
    fun `chainFromAbsoluteLocation should return relaychain when location has no junctions`() = runBlocking {
        val absoluteLocation = AbsoluteMultiLocation(GlobalConsensus(polkadotGenesis))

        val result = converter.chainFromAbsoluteLocation(absoluteLocation)

        assertEquals(polkadot, result)
    }

    @Test
    fun `chainFromAbsoluteLocation should return parachain when location has parachain junction`() = runBlocking {
        val absoluteLocation = AbsoluteMultiLocation(GlobalConsensus(polkadotGenesis), ParachainId(paraId))

        val result = converter.chainFromAbsoluteLocation(absoluteLocation)

        assertEquals(pah, result)
    }

    @Test
    fun `chainFromAbsoluteLocation should return null when parachain not found`() = runBlocking {
        val unknownParaId = 9999
        val absoluteLocation = AbsoluteMultiLocation(GlobalConsensus(polkadotGenesis), ParachainId(unknownParaId))

        val result = converter.chainFromAbsoluteLocation(absoluteLocation)

        assertNull(result)
    }

    @Test
    fun `chainFromAbsoluteLocation should return null when global consensus is missing`() = runBlocking {
        val absoluteLocation = AbsoluteMultiLocation(ParachainId(paraId))
        val result = converter.chainFromAbsoluteLocation(absoluteLocation)
        assertNull(result)
    }

    @Test
    fun `absoluteLocationFromChain should return global consensus for relay`() = runBlocking {
        val expected = AbsoluteMultiLocation(GlobalConsensus(polkadotGenesis))

        val result = converter.absoluteLocationFromChain(polkadot)

        assertEquals(expected, result)
    }

    @Test
    fun `absoluteLocationFromChain should return parachain location for parachain`() = runBlocking {
        val expected = AbsoluteMultiLocation(GlobalConsensus(polkadotGenesis), ParachainId(paraId))

        val result = converter.absoluteLocationFromChain(pah)

        assertEquals(expected, result)
    }
}
