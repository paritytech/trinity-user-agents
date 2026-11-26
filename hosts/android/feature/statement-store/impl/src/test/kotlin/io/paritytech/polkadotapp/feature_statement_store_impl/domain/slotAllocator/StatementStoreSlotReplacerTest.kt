@file:OptIn(ExperimentalTime::class)

package io.paritytech.polkadotapp.feature_statement_store_impl.domain.slotAllocator

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.novasama.substrate_sdk_android.encrypt.keypair.substrate.Sr25519Keypair
import io.paritytech.polkadotapp.bandersnatch_crypto.BandersnatchAlias
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.ChainId
import io.paritytech.polkadotapp.chains.repository.ChainStateRepository
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.common.utils.progressStallReport.StalenessReportCollector
import io.paritytech.polkadotapp.feature_account_api.domain.usecase.AccountDerivationUseCase
import io.paritytech.polkadotapp.feature_people_api.domain.PeopleCollection
import io.paritytech.polkadotapp.feature_statement_store_api.domain.slotAllocator.CollectionSlot
import io.paritytech.polkadotapp.feature_statement_store_api.domain.slotAllocator.StatementStoreSlot
import io.paritytech.polkadotapp.feature_statement_store_api.domain.slotAllocator.StatementStoreSlotAllocationError
import io.paritytech.polkadotapp.feature_statement_store_impl.data.repository.StatementStoreSlotAllocationRepository
import io.paritytech.polkadotapp.feature_statement_store_impl.data.signer.origins.StatementStoreOrigins
import io.paritytech.polkadotapp.feature_transactions.api.data.EnrichedSendableExtrinsic
import io.paritytech.polkadotapp.feature_transactions.api.data.ExtrinsicDispatch
import io.paritytech.polkadotapp.feature_transactions.api.data.ExtrinsicExecutionResult
import io.paritytech.polkadotapp.feature_transactions.api.data.ExtrinsicService
import io.paritytech.polkadotapp.feature_transactions.api.data.validation.ExtrinsicValidator
import io.paritytech.polkadotapp.feature_transactions.api.data.validation.InvalidTransaction
import io.paritytech.polkadotapp.feature_transactions.api.data.validation.TransactionValidity
import io.paritytech.polkadotapp.feature_transactions.api.data.validation.UnknownTransaction
import io.paritytech.polkadotapp.feature_transactions.api.domain.model.TransactionOrigin
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import kotlin.time.ExperimentalTime
import kotlin.time.Instant

class StatementStoreSlotReplacerTest {
    private val chainId: ChainId = "people"
    private val chain: Chain = mockk()
    private val context = AllocateContext(chain, listOf(PeopleCollection.People), period = 100u)
    private val blockHash = "0xblock"
    private val newAccount: AccountId = byteArrayOf(0x01).toDataByteArray()

    private val holderA = SlotHolder(seq = 0u, accountByte = 0x0A)
    private val holderB = SlotHolder(seq = 1u, accountByte = 0x0B)

    private val extrinsicService: ExtrinsicService = mockk()
    private val extrinsicValidator: ExtrinsicValidator = mockk()
    private val chainStateRepository: ChainStateRepository = mockk()
    private val origins: StatementStoreOrigins = mockk()
    private val accountDerivationUseCase: AccountDerivationUseCase = mockk()
    private val allocationRepository: StatementStoreSlotAllocationRepository = mockk()
    private val slotLoader: StatementStoreSlotLoader = mockk()

    private val replacer = StatementStoreSlotReplacer(
        extrinsicService = extrinsicService,
        extrinsicValidator = extrinsicValidator,
        chainStateRepository = chainStateRepository,
        statementStoreOrigins = origins,
        accountDerivationUseCase = accountDerivationUseCase,
        allocationRepository = allocationRepository,
        slotLoader = slotLoader,
    )

    @Before
    fun setUp() {
        every { chain.id } returns chainId
        coEvery { chainStateRepository.currentBlockHash(chainId) } returns blockHash
        coEvery { slotLoader.slotAlias(any(), any(), any()) } returns Result.success(BandersnatchAlias(byteArrayOf(0x7F)))
    }

    @Test
    fun `valid dry run submits the replacement signed by the holder and returns its slot`() = runBlocking<Unit> {
        withKeysOnDevice(holderA)
        withDryRun(holderA, Result.success(TransactionValidity.Valid))
        withSubmissionSucceeds(holderA)

        val result = replace(holderA)

        assertEquals(holderA.slot, result.getOrThrow())
        coVerify(exactly = 1) { extrinsicService.submitExtrinsicAndAwaitExecution(chain, holderA.origin, any(), any(), any()) }
    }

    @Test
    fun `slot whose holder has no stored derivation path is skipped`() = runBlocking<Unit> {
        withKeysOnDevice(holderB)
        withDryRun(holderB, Result.success(TransactionValidity.Valid))
        withSubmissionSucceeds(holderB)

        val result = replace(holderA, holderB)

        assertEquals(holderB.slot, result.getOrThrow())
    }

    @Test
    fun `slot whose stored path derives another key is skipped`() = runBlocking<Unit> {
        withSignerPaths(holderA, holderB)
        coEvery { accountDerivationUseCase.deriveKeypair(holderA.derivationPath) } returns Result.success(keypairWithPublicKey(byteArrayOf(0x55)))
        withHolderKey(holderB)
        withDryRun(holderB, Result.success(TransactionValidity.Valid))
        withSubmissionSucceeds(holderB)

        val result = replace(holderA, holderB)

        assertEquals(holderB.slot, result.getOrThrow())
    }

    @Test
    fun `no slot held by a key on this device fails with NoAllocationAvailable`() = runBlocking<Unit> {
        withSignerPaths()

        val result = replace(holderA)

        assertTrue(result.exceptionOrNull() is StatementStoreSlotAllocationError.NoAllocationAvailable)
        verifyNothingSubmitted()
    }

    @Test
    fun `slot that may not be replaced yet is passed over for the next one`() = runBlocking<Unit> {
        withKeysOnDevice(holderA, holderB)
        withDryRun(holderA, Result.success(TransactionValidity.Invalid(InvalidTransaction.Custom(228u))))
        withDryRun(holderB, Result.success(TransactionValidity.Valid))
        withSubmissionSucceeds(holderB)

        val result = replace(holderA, holderB)

        assertEquals(holderB.slot, result.getOrThrow())
        coVerify(exactly = 0) { extrinsicService.submitExtrinsicAndAwaitExecution(chain, holderA.origin, any(), any(), any()) }
    }

    @Test
    fun `no slot that may be replaced yet fails with EvictionNotYetAllowedException`() = runBlocking<Unit> {
        withKeysOnDevice(holderA, holderB)
        withDryRun(holderA, Result.success(TransactionValidity.Invalid(InvalidTransaction.Payment)))
        withDryRun(holderB, Result.success(TransactionValidity.Invalid(InvalidTransaction.Custom(228u))))

        val result = replace(holderA, holderB)

        assertTrue(result.exceptionOrNull() is EvictionNotYetAllowedException)
        verifyNothingSubmitted()
    }

    @Test
    fun `rejected dry run fails without trying the next slot`() = runBlocking<Unit> {
        withKeysOnDevice(holderA, holderB)
        withDryRun(holderA, Result.success(TransactionValidity.Invalid(InvalidTransaction.BadProof)))
        withDryRun(holderB, Result.success(TransactionValidity.Valid))
        withSubmissionSucceeds(holderB)

        val result = replace(holderA, holderB)

        val error = result.exceptionOrNull()
        assertTrue(error is SlotReplacementRejectedException)
        assertEquals(InvalidTransaction.BadProof, (error as SlotReplacementRejectedException).reason)
        verifyNothingSubmitted()
    }

    @Test
    fun `failed dry run call still submits the replacement`() = runBlocking<Unit> {
        withKeysOnDevice(holderA)
        withDryRun(holderA, Result.failure(IllegalStateException("runtime call failed")))
        withSubmissionSucceeds(holderA)

        val result = replace(holderA)

        assertEquals(holderA.slot, result.getOrThrow())
    }

    @Test
    fun `unknown validity still submits the replacement`() = runBlocking<Unit> {
        withKeysOnDevice(holderA)
        withDryRun(holderA, Result.success(TransactionValidity.Unknown(UnknownTransaction.CannotLookup)))
        withSubmissionSucceeds(holderA)

        val result = replace(holderA)

        assertEquals(holderA.slot, result.getOrThrow())
    }

    @Test
    fun `failed submission fails without trying the next slot`() = runBlocking<Unit> {
        val submissionError = IllegalStateException("submission failed")
        withKeysOnDevice(holderA, holderB)
        withDryRun(holderA, Result.success(TransactionValidity.Valid))
        withDryRun(holderB, Result.success(TransactionValidity.Valid))
        coEvery {
            extrinsicService.submitExtrinsicAndAwaitExecution(chain, holderA.origin, any(), any(), any())
        } returns Result.failure(submissionError)
        withSubmissionSucceeds(holderB)

        val result = replace(holderA, holderB)

        assertSame(submissionError, result.exceptionOrNull())
        coVerify(exactly = 0) { extrinsicService.submitExtrinsicAndAwaitExecution(chain, holderB.origin, any(), any(), any()) }
    }

    private fun withKeysOnDevice(vararg holders: SlotHolder) {
        withSignerPaths(*holders)
        holders.forEach(::withHolderKey)
    }

    private fun withSignerPaths(vararg holders: SlotHolder) {
        coEvery { allocationRepository.signerDerivationPathsFor(chainId, any()) } returns holders.associate { it.accountId to it.derivationPath }
    }

    private fun withHolderKey(holder: SlotHolder) {
        val keypair = keypairWithPublicKey(holder.accountId.value)
        coEvery { accountDerivationUseCase.deriveKeypair(holder.derivationPath) } returns Result.success(keypair)
        every { origins.asResourcesSlotHolder(keypair) } returns holder.origin
    }

    private fun withDryRun(holder: SlotHolder, validity: Result<TransactionValidity>) {
        val body = byteArrayOf(holder.accountId.value.single(), 0x00)
        val extrinsic: EnrichedSendableExtrinsic = mockk { every { bytesWithoutLength } returns body }
        coEvery { extrinsicService.buildExtrinsic(chain, holder.origin, any(), any()) } returns Result.success(extrinsic)
        coEvery { extrinsicValidator.validate(chainId, body.toDataByteArray(), blockHash) } returns validity
    }

    private fun withSubmissionSucceeds(holder: SlotHolder) {
        coEvery {
            extrinsicService.submitExtrinsicAndAwaitExecution(chain, holder.origin, any(), any(), any())
        } returns Result.success(ExtrinsicExecutionResult("0xext", blockHash, ExtrinsicDispatch.Ok(emptyList())))
    }

    private fun verifyNothingSubmitted() {
        coVerify(exactly = 0) { extrinsicService.submitExtrinsicAndAwaitExecution(any(), any(), any(), any(), any()) }
    }

    private suspend fun replace(vararg holders: SlotHolder): Result<CollectionSlot<StatementStoreSlot.Taken>> {
        return with(StalenessReportCollector.NoOp) {
            replacer.replaceFirstAvailable(context, holders.map { it.slot }, newAccount)
        }
    }

    private fun keypairWithPublicKey(publicKey: ByteArray) = Sr25519Keypair(
        privateKey = ByteArray(32),
        publicKey = publicKey,
        nonce = ByteArray(32),
    )

    private class SlotHolder(seq: UInt, accountByte: Int) {
        val accountId: AccountId = byteArrayOf(accountByte.toByte()).toDataByteArray()
        val derivationPath = "//holder/$seq"
        val origin: TransactionOrigin = mockk()
        val slot = CollectionSlot(
            collection = PeopleCollection.People,
            slot = StatementStoreSlot.Taken(seq = seq, accountId = accountId, since = Instant.fromEpochSeconds(1_000)),
        )
    }
}
