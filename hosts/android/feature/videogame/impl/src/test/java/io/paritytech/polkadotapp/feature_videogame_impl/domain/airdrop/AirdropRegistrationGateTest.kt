package io.paritytech.polkadotapp.feature_videogame_impl.domain.airdrop

import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.storage.source.query.AtBlock
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_videogame_api.domain.state.model.GameIndex
import io.paritytech.polkadotapp.feature_videogame_impl.data.gameResults.AirdropRepository
import io.paritytech.polkadotapp.feature_videogame_impl.data.models.onchain.OnChainActiveEvent
import io.paritytech.polkadotapp.feature_videogame_impl.data.models.onchain.OnChainAirdropEventInfo
import io.paritytech.polkadotapp.feature_videogame_impl.data.models.onchain.OnChainAirdropPrize
import io.paritytech.polkadotapp.feature_videogame_impl.data.models.onchain.OnChainAirdropStatus
import io.paritytech.polkadotapp.feature_videogame_impl.data.models.onchain.OnChainVideoGameInfo
import io.paritytech.polkadotapp.feature_videogame_impl.data.models.onchain.OnChainVideoGameState
import io.paritytech.polkadotapp.feature_videogame_impl.data.repositories.VideoGameRepositoryInternal
import io.paritytech.polkadotapp.feature_xcm_api.multiLocation.MultiLocation
import io.paritytech.polkadotapp.feature_xcm_api.multiLocation.RelativeMultiLocation
import junit.framework.TestCase.assertFalse
import junit.framework.TestCase.assertTrue
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.runBlocking
import org.junit.Test
import java.math.BigInteger

/**
 * Unit tests for the register-gate logic. Non-airdrop games are always open; an airdrop game stays
 * gated until its event reaches `Registering`; a failed status read stays gated.
 */
class AirdropRegistrationGateTest {
    private val chainRegistry: ChainRegistry = mockk()
    private val videoGameRepository: VideoGameRepositoryInternal = mockk()
    private val airdropRepository: AirdropRepository = mockk()

    private val gate = AirdropRegistrationGate(chainRegistry, videoGameRepository, airdropRepository)

    private val peopleChain: Chain = mockk<Chain>().also { every { it.id } returns CHAIN_ID }

    @Test
    fun `non-airdrop game is always open`() = runBlocking<Unit> {
        stubGame(airdropScheduled = false)

        assertTrue(firstGate())
    }

    @Test
    fun `airdrop game is gated while the event is not registering`() = runBlocking<Unit> {
        stubGame(airdropScheduled = true)
        stubStatus(OnChainAirdropStatus.Scheduled)

        assertFalse(firstGate())
    }

    @Test
    fun `airdrop game opens once the event is registering`() = runBlocking<Unit> {
        stubGame(airdropScheduled = true)
        stubStatus(OnChainAirdropStatus.Registering(totalParticipants = 5))

        assertTrue(firstOpen())
    }

    @Test
    fun `airdrop game stays gated when the status read fails`() = runBlocking<Unit> {
        stubGame(airdropScheduled = true)
        every { airdropRepository.subscribeActiveEvent(any(), any()) } returns
            flow { throw RuntimeException("rpc down") }

        assertFalse(firstGate())
    }

    // The flow is cold, so it must be built and collected inside the same runBlocking that owns the
    // ComputationalScope (mirrors RealGameResultsInteractorTest).
    private fun firstGate(): Boolean = runBlocking {
        val scope = ComputationalScope(this)
        with(scope) { gate.subscribe() }.first()
    }

    private fun firstOpen(): Boolean = runBlocking {
        val scope = ComputationalScope(this)
        with(scope) { gate.subscribe() }.first { it }
    }

    private fun stubGame(airdropScheduled: Boolean) {
        coEvery { chainRegistry.peopleChain() } returns peopleChain
        every {
            with(any<ComputationalScope>()) { videoGameRepository.subscribeGameInfoAtBlock(any()) }
        } returns flowOf(AtBlock(value = gameInfo(airdropScheduled), at = "0xblock"))
    }

    private fun stubStatus(status: OnChainAirdropStatus) {
        every { airdropRepository.subscribeActiveEvent(any(), any()) } returns flowOf(activeEvent(status))
    }

    private fun activeEvent(status: OnChainAirdropStatus) = OnChainActiveEvent(
        id = byteArrayOf(1).toDataByteArray(),
        info = OnChainAirdropEventInfo(
            prize = OnChainAirdropPrize(
                assetId = RelativeMultiLocation(parents = 0, interior = MultiLocation.Interior.Here),
                assetAmount = BigInteger.ONE,
                maxWinners = 1,
                winnerCap = 1,
            ),
            registrationStarts = 0L,
            drawTime = 0L,
            endTime = 0L,
        ),
        status = status,
    )

    private fun gameInfo(airdropScheduled: Boolean) = OnChainVideoGameInfo(
        index = GameIndex(1),
        registrationEnds = 0L,
        gameDate = 0L,
        reportEnds = 0L,
        maxGroupSize = 4,
        rounds = 1,
        state = OnChainVideoGameState.PlayerProcess,
        airdropsScheduled = if (airdropScheduled) 1 else 0,
    )

    private companion object {
        const val CHAIN_ID = "people-chain-id"
    }
}
