package io.paritytech.polkadotapp.feature_connection_status_impl.data

import io.paritytech.polkadotapp.chains.network.binding.BlockNumber
import io.paritytech.polkadotapp.chains.network.binding.toBlockNumber
import io.paritytech.polkadotapp.chains.network.rpc.RpcCalls
import io.paritytech.polkadotapp.chains.network.rpc.model.SignedBlock
import io.paritytech.polkadotapp.chains.storage.source.StorageDataSource
import io.paritytech.polkadotapp.test_shared.any
import io.paritytech.polkadotapp.test_shared.argThat
import io.paritytech.polkadotapp.test_shared.eq
import io.paritytech.polkadotapp.test_shared.whenever
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.mockito.Mockito.mock
import org.mockito.Mockito.verify
import java.math.BigInteger
import kotlin.time.Duration
import kotlin.time.Duration.Companion.seconds

class ChainAnchorDataSourceTest {
    private val rpcCalls: RpcCalls = mock(RpcCalls::class.java)
    private val remoteStorage: StorageDataSource = mock(StorageDataSource::class.java)

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

        verify(rpcCalls).getBlockHeader(eq(CHAIN_ID), eq(HEAD_HASH))
        verify(rpcCalls).getBlockHash(eq(CHAIN_ID), eqBlockNumber((HEAD_HEIGHT - EXPECTED_BLOCKS).toBlockNumber()))
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
        whenever(rpcCalls.getBlockHash(CHAIN_ID)).thenReturn(HEAD_HASH)
        whenever(rpcCalls.getBlockHeader(eq(CHAIN_ID), eq(HEAD_HASH))).thenReturn(header(headHeight))
        whenever(rpcCalls.getBlockHash(eq(CHAIN_ID), eqBlockNumber((headHeight - EXPECTED_BLOCKS).toBlockNumber())))
            .thenReturn(PREVIOUS_HASH)
        withTimestamp(at = HEAD_HASH, millis = PREVIOUS_MILLIS + span.inWholeMilliseconds)
        withTimestamp(at = PREVIOUS_HASH, millis = PREVIOUS_MILLIS)
    }

    private suspend fun withTimestamp(at: String, millis: Long) {
        whenever(remoteStorage.query<BigInteger>(eq(CHAIN_ID), eq(at), any())).thenReturn(millis.toBigInteger())
    }

    private fun header(height: Int) = SignedBlock.Block.Header(height.toString(radix = 16), parentHash = null)

    // BlockNumber is a value class: Mockito's own matchers unbox null into it and NPE, and the argument it
    // records is the unwrapped number. test-shared cannot carry this matcher because it does not see :chains.
    private fun eqBlockNumber(value: BlockNumber): BlockNumber {
        argThat<Any?> { actual -> actual?.toString() == value.value.toString() }
        return value
    }

    private companion object {
        const val CHAIN_ID = "people"
        const val HEAD_HASH = "0xhead"
        const val PREVIOUS_HASH = "0xprev"
        const val EXPECTED_BLOCKS = 15
        const val HEAD_HEIGHT = 100
        const val PREVIOUS_MILLIS = 270_000L
    }
}
