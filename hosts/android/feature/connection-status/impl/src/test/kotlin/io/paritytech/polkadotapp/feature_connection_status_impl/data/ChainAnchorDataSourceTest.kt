package io.paritytech.polkadotapp.feature_connection_status_impl.data

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.mockk
import io.paritytech.polkadotapp.chains.network.binding.toBlockNumber
import io.paritytech.polkadotapp.chains.network.rpc.RpcCalls
import io.paritytech.polkadotapp.chains.network.rpc.model.SignedBlock
import io.paritytech.polkadotapp.chains.storage.source.StorageDataSource
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.math.BigInteger
import kotlin.time.Duration
import kotlin.time.Duration.Companion.seconds

class ChainAnchorDataSourceTest {
    private val rpcCalls: RpcCalls = mockk()
    private val remoteStorage: StorageDataSource = mockk()

    private val dataSource = ChainAnchorDataSource(rpcCalls, remoteStorage)

    @Test
    fun `the span is the chain time across the blocks the window expects`() = runBlocking<Unit> {
        withChain(headHeight = HEAD_HEIGHT, span = 30.seconds)

        val anchor = dataSource.fetch(CHAIN_ID, EXPECTED_BLOCKS).getOrThrow()

        assertEquals(HEAD_HEIGHT, anchor.headHeight)
        assertEquals(EXPECTED_BLOCKS, anchor.blocks)
        assertEquals(30.seconds, anchor.chainClockSpan)
    }

    @Test
    fun `the block reached back to is counted off the height the head hash resolved to`() = runBlocking<Unit> {
        withChain(headHeight = HEAD_HEIGHT, span = 30.seconds)

        dataSource.fetch(CHAIN_ID, EXPECTED_BLOCKS)

        coVerify(exactly = 1) { rpcCalls.getBlockHeader(CHAIN_ID, HEAD_HASH) }
        coVerify(exactly = 1) { rpcCalls.getBlockHash(CHAIN_ID, (HEAD_HEIGHT - EXPECTED_BLOCKS).toBlockNumber()) }
    }

    @Test
    fun `a chain shorter than the window is rejected`() = runBlocking<Unit> {
        withChain(headHeight = EXPECTED_BLOCKS - 1, span = 30.seconds)

        val anchor = dataSource.fetch(CHAIN_ID, EXPECTED_BLOCKS)

        assertTrue("a chain shorter than the window anchored to $anchor", anchor.isFailure)
    }

    @Test
    fun `timestamps that do not move forward are rejected`() = runBlocking<Unit> {
        withChain(headHeight = HEAD_HEIGHT, span = Duration.ZERO)

        val anchor = dataSource.fetch(CHAIN_ID, EXPECTED_BLOCKS)

        assertTrue("a chain whose clock did not move anchored to $anchor", anchor.isFailure)
    }

    private suspend fun withChain(headHeight: Int, span: Duration) {
        coEvery { rpcCalls.getBlockHash(CHAIN_ID) } returns HEAD_HASH
        coEvery { rpcCalls.getBlockHeader(CHAIN_ID, HEAD_HASH) } returns header(headHeight)
        coEvery { rpcCalls.getBlockHash(CHAIN_ID, (headHeight - EXPECTED_BLOCKS).toBlockNumber()) } returns PREVIOUS_HASH
        withTimestamp(at = HEAD_HASH, millis = PREVIOUS_MILLIS + span.inWholeMilliseconds)
        withTimestamp(at = PREVIOUS_HASH, millis = PREVIOUS_MILLIS)
    }

    private suspend fun withTimestamp(at: String, millis: Long) {
        coEvery { remoteStorage.query<BigInteger>(CHAIN_ID, at, any()) } returns millis.toBigInteger()
    }

    private fun header(height: Int) = SignedBlock.Block.Header(height.toString(radix = 16), parentHash = null)

    private companion object {
        const val CHAIN_ID = "people"
        const val HEAD_HASH = "0xhead"
        const val PREVIOUS_HASH = "0xprev"
        const val EXPECTED_BLOCKS = 15
        const val HEAD_HEIGHT = 100
        const val PREVIOUS_MILLIS = 270_000L
    }
}
