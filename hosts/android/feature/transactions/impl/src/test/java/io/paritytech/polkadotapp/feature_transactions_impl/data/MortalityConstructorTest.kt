package io.paritytech.polkadotapp.feature_transactions_impl.data

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.mockk
import io.paritytech.polkadotapp.chains.network.rpc.RpcCalls
import io.paritytech.polkadotapp.chains.network.rpc.model.SignedBlock
import io.paritytech.polkadotapp.chains.repository.ChainStateRepository
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.test_shared.testDispatchers
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test
import java.math.BigInteger
import kotlin.time.Duration.Companion.seconds

class MortalityConstructorTest {
    // A light client answers a block-hash lookup by number with null for every block but the genesis, so a
    // transaction anchored through one could never be signed.
    @Test
    fun `the era is anchored at the finalized block without looking a hash up by number`() = runTest {
        val rpcCalls = mockk<RpcCalls> {
            coEvery { getFinalizedHead(CHAIN) } returns FINALIZED_HASH
            coEvery { getBlockHeader(CHAIN, null) } returns header(number = 105, parentHash = BEST_PARENT_HASH)
            coEvery { getBlockHeader(CHAIN, BEST_PARENT_HASH) } returns header(number = 104, parentHash = null)
            coEvery { getBlockHeader(CHAIN, FINALIZED_HASH) } returns header(number = 100, parentHash = null)
        }
        val chainState = mockk<ChainStateRepository> {
            coEvery { blockHashCount(CHAIN) } returns BigInteger.valueOf(4096)
            coEvery { expectedBlockTime(CHAIN) } returns 6.seconds
        }

        val mortality = MortalityConstructor(chainState, testDispatchers()).constructMortality(CHAIN, rpcCalls)

        assertEquals(DataByteArray.fromHex(FINALIZED_HASH), mortality.blockHash)
        coVerify(exactly = 0) { rpcCalls.getBlockHash(any(), any()) }
    }

    private fun header(number: Int, parentHash: String?) =
        SignedBlock.Block.Header(numberRaw = "0x" + number.toString(16), parentHash = parentHash)

    private companion object {
        const val CHAIN = "chain"
        val FINALIZED_HASH = "0x" + "ab".repeat(32)
        val BEST_PARENT_HASH = "0x" + "cd".repeat(32)
    }
}
