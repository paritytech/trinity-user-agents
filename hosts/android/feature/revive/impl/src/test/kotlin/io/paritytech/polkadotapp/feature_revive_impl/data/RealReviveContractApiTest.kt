package io.paritytech.polkadotapp.feature_revive_impl.data

import io.mockk.coEvery
import io.mockk.mockk
import io.paritytech.polkadotapp.chains.call.MultiChainRuntimeCallsApi
import io.paritytech.polkadotapp.chains.call.RuntimeCallsApi
import io.paritytech.polkadotapp.chains.network.binding.ScaleResult
import io.paritytech.polkadotapp.chains.network.binding.WeightV2
import io.paritytech.polkadotapp.chains.storage.source.StorageDataSource
import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import io.paritytech.polkadotapp.common.domain.model.intoAccountId
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_revive_api.ReviveContractReverted
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.math.BigInteger

class RealReviveContractApiTest {
    private var response: Any? = null

    // The runtime is left unstubbed: only a dispatch error needs it, and touching it on success would throw.
    private val calls = mockk<RuntimeCallsApi> {
        coEvery { call<Any?>(any(), any(), any(), any(), any()) } answers { response }
    }
    private val multiChain = mockk<MultiChainRuntimeCallsApi> {
        coEvery { forChain(CHAIN) } returns calls
    }

    private val api = RealReviveContractApi(
        multiChainRuntimeCallsApi = multiChain,
        remoteStorageSource = mockk<StorageDataSource>(),
    )

    @Test
    fun `a dry-run whose contract reverted fails with the revert data`() = runTest {
        response = dryRunResult(flags = REVERT_FLAG, data = REVERT_DATA)

        val error = api.dryRun(CHAIN, ORIGIN, CONTRACT, INPUT).exceptionOrNull()

        assertTrue("expected a revert, got $error", error is ReviveContractReverted)
        assertEquals(REVERT_DATA, (error as ReviveContractReverted).data)
    }

    @Test
    fun `a dry-run that returned normally reports its output and limits`() = runTest {
        response = dryRunResult(flags = 0, data = OUTPUT)

        val dryRun = api.dryRun(CHAIN, ORIGIN, CONTRACT, INPUT).getOrThrow()

        assertEquals(OUTPUT, dryRun.data)
        assertEquals(WEIGHT, dryRun.weightRequired)
        assertEquals(DEPOSIT, dryRun.storageDeposit)
    }

    @Test
    fun `a read whose contract reverted fails with the revert data`() = runTest {
        response = ReviveContractResult(ScaleResult.Ok(ExecReturnValue(ReturnFlags(REVERT_FLAG), REVERT_DATA)))

        val error = api.callReadOnly(CHAIN, CONTRACT, INPUT).exceptionOrNull()

        assertTrue("expected a revert, got $error", error is ReviveContractReverted)
    }

    @Test
    fun `an unavailable chain fails the dry-run rather than throwing`() = runTest {
        coEvery { multiChain.forChain(CHAIN) } throws IllegalStateException("runtime not synced")

        assertTrue(api.dryRun(CHAIN, ORIGIN, CONTRACT, INPUT).isFailure)
    }

    @Test
    fun `an unavailable chain fails the read rather than throwing`() = runTest {
        coEvery { multiChain.forChain(CHAIN) } throws IllegalStateException("runtime not synced")

        assertTrue(api.callReadOnly(CHAIN, CONTRACT, INPUT).isFailure)
    }

    private fun dryRunResult(flags: Int, data: DataByteArray) = ReviveDryRunResult(
        weightRequired = WEIGHT,
        storageDeposit = ReviveStorageDeposit.Charge(DEPOSIT),
        result = ScaleResult.Ok(ExecReturnValue(ReturnFlags(flags), data)),
    )

    private companion object {
        const val CHAIN = "asset-hub"
        const val REVERT_FLAG = 0x1

        val ORIGIN = ByteArray(32) { 0x01 }.intoAccountId()
        val CONTRACT = ByteArray(20) { 0x0c }.toDataByteArray()
        val INPUT = byteArrayOf(0x74, 0x02, 0x04, 0xc6.toByte()).toDataByteArray()
        val OUTPUT = byteArrayOf(0x0a).toDataByteArray()
        val REVERT_DATA = byteArrayOf(0x00, 0x30, 0x5b, 0x93.toByte()).toDataByteArray()
        val WEIGHT = WeightV2(refTime = BigInteger.valueOf(1_000), proofSize = BigInteger.valueOf(64))
        val DEPOSIT: BigInteger = BigInteger.valueOf(413_000_000)
    }
}
