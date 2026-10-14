package io.paritytech.polkadotapp.feature_coinage_impl.domain.installation

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.coVerifyOrder
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.network.binding.WeightV2
import io.paritytech.polkadotapp.chains.network.binding.intoBalance
import io.paritytech.polkadotapp.common.domain.model.AeadKey
import io.paritytech.polkadotapp.common.domain.model.intoAccountId
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_coinage_impl.TEST_INSTALLATION
import io.paritytech.polkadotapp.feature_coinage_impl.data.dataStore.AccountDataStoreRepository
import io.paritytech.polkadotapp.feature_coinage_impl.data.dataStore.DataStoreAccount
import io.paritytech.polkadotapp.feature_coinage_impl.data.dataStore.InstallationRegistrationCall
import io.paritytech.polkadotapp.feature_revive_api.ReviveContractApi
import io.paritytech.polkadotapp.feature_revive_api.ReviveContractReverted
import io.paritytech.polkadotapp.feature_revive_api.ReviveDryRun
import io.paritytech.polkadotapp.feature_transactions.api.data.EnrichedSendableExtrinsic
import io.paritytech.polkadotapp.feature_transactions.api.data.ExtrinsicService
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTransactionService
import io.paritytech.polkadotapp.feature_transactions.api.domain.durable.DurableTxId
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.AccountFee
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import java.math.BigInteger

class RealInstallationRegistrationSubmitterTest {
    private val chain = mockk<Chain> { every { id } returns ASSET_HUB }
    private val extrinsic = mockk<EnrichedSendableExtrinsic>()
    private val chainRegistry = mockk<ChainRegistry>()
    private val dataStoreRepository = mockk<AccountDataStoreRepository>()
    private val reviveContractApi = mockk<ReviveContractApi>()
    private val pgasProvisioner = mockk<DataStorePgasProvisioner>()
    private val extrinsicService = mockk<ExtrinsicService>()
    private val durableTransactionService = mockk<DurableTransactionService>()

    private val submitter = RealInstallationRegistrationSubmitter(
        chainRegistry = chainRegistry,
        knownChains = KnownChains(people = "people", assetHub = ASSET_HUB, bulletIn = "bullet-in", hydration = null),
        dataStoreRepository = dataStoreRepository,
        reviveContractApi = reviveContractApi,
        pgasProvisioner = pgasProvisioner,
        extrinsicService = extrinsicService,
        durableTransactionService = durableTransactionService,
    )

    @Before
    fun setUp() {
        val account = DataStoreAccount(keypair = mockk(), accountId = ACCOUNT_ID, evmAccountId = EVM_ACCOUNT, encryptionKey = AeadKey.fromDerivedBytes(ByteArray(AeadKey.SIZE_BYTES)))
        val fee = mockk<AccountFee> { every { amount } returns FEE }

        coEvery { chainRegistry.getChain(ASSET_HUB) } returns chain
        coEvery { dataStoreRepository.registrationCall(TARGET) } returns Result.success(InstallationRegistrationCall(account, CONTRACT, INPUT))
        coEvery { pgasProvisioner.ensureFunded(ACCOUNT_ID) } returns Result.success(Unit)
        coEvery { reviveContractApi.isAccountMapped(ASSET_HUB, ACCOUNT_ID) } returns Result.success(true)
        coEvery { reviveContractApi.dryRun(ASSET_HUB, ACCOUNT_ID, CONTRACT, INPUT) } returns Result.success(DRY_RUN)
        coEvery { extrinsicService.estimateFee(chain, any(), any(), any()) } returns Result.success(fee)
        coEvery { pgasProvisioner.ensureCovers(ACCOUNT_ID, any()) } returns Result.success(Unit)
        coEvery { extrinsicService.buildExtrinsic(chain, any(), any(), any()) } returns Result.success(extrinsic)
        coEvery {
            durableTransactionService.submit(COINAGE_INSTALLATION_DOMAIN, extrinsic, TARGET.registrationGroup(), any())
        } returns Result.success(SUBMITTED)
    }

    @Test
    fun `an attempt is funded, checked, simulated and handed to the engine under its target's group`() = runTest {
        val result = submitter.submitAttempt(TARGET)

        assertEquals(SUBMITTED, result.getOrNull())
        coVerifyOrder {
            pgasProvisioner.ensureFunded(ACCOUNT_ID)
            reviveContractApi.isAccountMapped(ASSET_HUB, ACCOUNT_ID)
            reviveContractApi.dryRun(ASSET_HUB, ACCOUNT_ID, CONTRACT, INPUT)
            pgasProvisioner.ensureCovers(ACCOUNT_ID, any())
            extrinsicService.buildExtrinsic(chain, any(), any(), any())
            durableTransactionService.submit(COINAGE_INSTALLATION_DOMAIN, extrinsic, TARGET.registrationGroup(), any())
        }
    }

    @Test
    fun `the balance required is the storage deposit with its margin plus the fee`() = runTest {
        submitter.submitAttempt(TARGET)

        coVerify { pgasProvisioner.ensureCovers(ACCOUNT_ID, (DEPOSIT * BigInteger.valueOf(120) / BigInteger.valueOf(100) + FEE.value).intoBalance()) }
    }

    @Test
    fun `an unmapped data store account fails the attempt before anything is signed`() = runTest {
        coEvery { reviveContractApi.isAccountMapped(ASSET_HUB, ACCOUNT_ID) } returns Result.success(false)

        val result = submitter.submitAttempt(TARGET)

        assertTrue(result.exceptionOrNull() is DataStoreAccountUnmappedError)
        coVerify(exactly = 0) { reviveContractApi.dryRun(any(), any(), any(), any()) }
        coVerify(exactly = 0) { durableTransactionService.submit(any(), any(), any(), any()) }
    }

    @Test
    fun `an account that could not be funded is never simulated`() = runTest {
        coEvery { pgasProvisioner.ensureFunded(ACCOUNT_ID) } returns Result.failure(IllegalStateException("not in the ring"))

        val result = submitter.submitAttempt(TARGET)

        assertTrue(result.isFailure)
        coVerify(exactly = 0) { reviveContractApi.dryRun(any(), any(), any(), any()) }
    }

    @Test
    fun `a call the contract would revert is never submitted`() = runTest {
        coEvery { reviveContractApi.dryRun(ASSET_HUB, ACCOUNT_ID, CONTRACT, INPUT) } returns
            Result.failure(ReviveContractReverted(byteArrayOf(0x04).toDataByteArray()))

        val result = submitter.submitAttempt(TARGET)

        assertTrue(result.exceptionOrNull() is ReviveContractReverted)
        coVerify(exactly = 0) { durableTransactionService.submit(any(), any(), any(), any()) }
    }

    @Test
    fun `a balance that cannot cover the call is never submitted`() = runTest {
        coEvery { pgasProvisioner.ensureCovers(ACCOUNT_ID, any()) } returns
            Result.failure(DataStorePgasShortError(FEE, 1.intoBalance()))

        val result = submitter.submitAttempt(TARGET)

        assertTrue(result.exceptionOrNull() is DataStorePgasShortError)
        coVerify(exactly = 0) { durableTransactionService.submit(any(), any(), any(), any()) }
    }

    private companion object {
        const val ASSET_HUB = "asset-hub"

        val ACCOUNT_ID = ByteArray(32) { 0x0a }.intoAccountId()
        val EVM_ACCOUNT = ByteArray(20) { 0x0e }.toDataByteArray()
        val CONTRACT = ByteArray(20) { 0x0c }.toDataByteArray()
        val INPUT = byteArrayOf(0xe5.toByte(), 0x61, 0x86.toByte(), 0x8d.toByte()).toDataByteArray()
        val TARGET = InstallationRegistrationTarget(CONTRACT, TEST_INSTALLATION)
        val DEPOSIT: BigInteger = BigInteger.valueOf(413_000_000)
        val FEE = 30_000_000.intoBalance()
        val SUBMITTED = DurableTxId(7)
        val DRY_RUN = ReviveDryRun(
            data = byteArrayOf().toDataByteArray(),
            weightRequired = WeightV2(refTime = BigInteger.valueOf(1_000_000), proofSize = BigInteger.valueOf(70_000)),
            storageDeposit = DEPOSIT,
        )
    }
}
