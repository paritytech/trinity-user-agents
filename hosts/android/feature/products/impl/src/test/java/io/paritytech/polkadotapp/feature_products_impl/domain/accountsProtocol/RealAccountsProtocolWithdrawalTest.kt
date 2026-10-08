package io.paritytech.polkadotapp.feature_products_impl.domain.accountsProtocol

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.mockk
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.feature_account_api.domain.derivation.DerivationIndex32
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.ApAllocatableResource
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.OnExistingAllowancePolicy
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.ProductProofContext
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.VrfTranscriptItem
import io.paritytech.polkadotapp.feature_products_api.model.ProductAccountId
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.signing.SigningContextHolder
import io.paritytech.polkadotapp.feature_products_impl.domain.crossProductProof.CrossProductProofContextHolder
import io.paritytech.polkadotapp.feature_products_impl.domain.resourceAllocationRequest.ResourceAllocationRequestContextHolder
import io.paritytech.polkadotapp.feature_products_impl.domain.signTransaction.ProductSigningScreenLauncher
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** A caller that stops waiting on a prompt withdraws it, so the prompt can close instead of going stale. */
class RealAccountsProtocolWithdrawalTest {
    private val router: ProductsRouter = mockk(relaxed = true)
    private val allocationHolder = ResourceAllocationRequestContextHolder()
    private val proofHolder = CrossProductProofContextHolder()
    private val signingHolder = SigningContextHolder()

    private val accountsProtocol = RealAccountsProtocol(
        contextHolder = allocationHolder,
        productsRouter = router,
        membersRingLocator = mockk(),
        membershipProver = mockk(),
        bandersnatchSecretsStorage = mockk(),
        ringVrfKeyRegistry = mockk(),
        ringVrfKeySource = mockk(),
        permissionGuard = mockk(),
        crossProductProofContextHolder = proofHolder,
        productSigningScreenLauncher = ProductSigningScreenLauncher(signingHolder, router),
    )

    private val callingProduct = ProductId.fromStoredValue("lottery.dot")

    @Test
    fun `the allocation prompt is withdrawn when its caller stops waiting`() = runTest {
        val caller = launch {
            accountsProtocol.requestResourceAllocation(callingProduct, listOf(ApAllocatableResource.BulletInAllowance), OnExistingAllowancePolicy.IGNORE)
        }
        runCurrent()
        val prompt = requireNotNull(allocationHolder.get())
        assertFalse(prompt.isWithdrawn)
        coVerify { router.openResourceAllocationRequestPrompt(prompt.id) }

        caller.cancel()
        runCurrent()

        assertTrue(prompt.isWithdrawn)
    }

    @Test
    fun `the proof prompt is withdrawn when its caller stops waiting`() = runTest {
        val onBehalfOf = ProductProofContext(ProductId.fromStoredValue("voting.dot"), DerivationIndex32.default())
        val caller = launch {
            accountsProtocol.createProof(callingProduct, ProductAccountId("voting.dot", DerivationIndex32.default()), onBehalfOf, mockk(), byteArrayOf(1))
        }
        runCurrent()
        val prompt = requireNotNull(proofHolder.get())
        assertFalse(prompt.isWithdrawn)
        coVerify { router.openCrossProductProofPrompt(prompt.id) }

        caller.cancel()
        runCurrent()

        assertTrue(prompt.isWithdrawn)
    }

    @Test
    fun `the VRF signing sheet is withdrawn when its caller stops waiting`() = runTest {
        val item = VrfTranscriptItem(label = "l".toByteArray().toDataByteArray(), value = byteArrayOf(1).toDataByteArray())
        val caller = launch {
            accountsProtocol.signVrf(callingProduct, ProductAccountId("lottery.dot", DerivationIndex32.default()), "label".toByteArray(), listOf(item))
        }
        runCurrent()
        val sheet = requireNotNull(signingHolder.get())
        var withdrawn = false
        launch { sheet.awaitWithdrawal().also { withdrawn = true } }
        runCurrent()
        assertFalse(withdrawn)

        caller.cancel()
        runCurrent()

        assertTrue(withdrawn)
    }

    /** The sheet may already be on screen when navigation hands back, so a caller cancelled mid-open still withdraws. */
    @Test
    fun `the allocation prompt is withdrawn when its caller stops waiting while it opens`() = runTest {
        coEvery { router.openResourceAllocationRequestPrompt(any()) } coAnswers { awaitCancellation() }
        val caller = launch {
            accountsProtocol.requestResourceAllocation(callingProduct, listOf(ApAllocatableResource.BulletInAllowance), OnExistingAllowancePolicy.IGNORE)
        }
        runCurrent()

        caller.cancel()
        runCurrent()

        assertTrue(requireNotNull(allocationHolder.get()).isWithdrawn)
    }

    @Test
    fun `the proof prompt is withdrawn when its caller stops waiting while it opens`() = runTest {
        coEvery { router.openCrossProductProofPrompt(any()) } coAnswers { awaitCancellation() }
        val onBehalfOf = ProductProofContext(ProductId.fromStoredValue("voting.dot"), DerivationIndex32.default())
        val caller = launch {
            accountsProtocol.createProof(callingProduct, ProductAccountId("voting.dot", DerivationIndex32.default()), onBehalfOf, mockk(), byteArrayOf(1))
        }
        runCurrent()

        caller.cancel()
        runCurrent()

        assertTrue(requireNotNull(proofHolder.get()).isWithdrawn)
    }

    @Test
    fun `the VRF signing sheet is withdrawn when its caller stops waiting while it opens`() = runTest {
        coEvery { router.openSignTransaction(any()) } coAnswers { awaitCancellation() }
        val item = VrfTranscriptItem(label = "l".toByteArray().toDataByteArray(), value = byteArrayOf(1).toDataByteArray())
        val caller = launch {
            accountsProtocol.signVrf(callingProduct, ProductAccountId("lottery.dot", DerivationIndex32.default()), "label".toByteArray(), listOf(item))
        }
        runCurrent()
        val sheet = requireNotNull(signingHolder.get())
        var withdrawn = false
        launch { sheet.awaitWithdrawal().also { withdrawn = true } }

        caller.cancel()
        runCurrent()

        assertTrue(withdrawn)
    }
}
