package io.paritytech.polkadotapp.feature_products_impl.presentation.resourceAllocationRequest

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.mockk
import io.mockk.verify
import io.paritytech.polkadotapp.common.utils.progressStallReport.StalenessReportCollector
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.ApAllocatableResource
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.ApAllocationOutcome
import io.paritytech.polkadotapp.feature_products_api.domain.accountsProtocol.OnExistingAllowancePolicy
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.resourceAllocationRequest.ResourceAllocationRequestContext
import io.paritytech.polkadotapp.feature_products_impl.domain.resourceAllocationRequest.ResourceAllocationRequestContextHolder
import io.paritytech.polkadotapp.feature_products_impl.domain.resourceAllocationRequest.ResourceAllocationRequestInteractor
import io.paritytech.polkadotapp.feature_products_impl.presentation.createAndClear
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Before
import org.junit.Test

class ResourceAllocationRequestViewModelTest {
    private val testDispatcher = StandardTestDispatcher()
    private val router: ProductsRouter = mockk(relaxed = true)
    private val interactor: ResourceAllocationRequestInteractor = mockk()

    private val holder = ResourceAllocationRequestContextHolder()

    private val context = allocationContext("lottery.dot")

    @Before
    fun setUp() = Dispatchers.setMain(testDispatcher)

    @After
    fun tearDown() = Dispatchers.resetMain()

    @Test
    fun `the allocation prompt closes when it is withdrawn before approval`() = runTest(testDispatcher) {
        viewModel()
        val caller = launch { context.awaitOutcomes {} }
        runCurrent()
        verifyPromptClosed(times = 0)

        caller.cancel()
        runCurrent()

        verifyPromptClosed(times = 1)
    }

    @Test
    fun `a withdrawn allocation prompt that was not on top closes when it is shown again`() = runTest(testDispatcher) {
        val model = viewModel()
        val caller = launch { context.awaitOutcomes {} }
        runCurrent()
        caller.cancel()
        runCurrent()
        verifyPromptClosed(times = 1)

        model.onResume()
        runCurrent()

        verifyPromptClosed(times = 2)
    }

    /** Leaving mid-allocation would cancel the steps already submitted, so the prompt stays until they end. */
    @Test
    fun `the allocation prompt closes only once the allocation ends when it is withdrawn mid-allocation`() = runTest(testDispatcher) {
        val allocationEnds = CompletableDeferred<Unit>()
        coEvery { with(any<StalenessReportCollector>()) { interactor.allocateAll(any(), any(), any(), any()) } } coAnswers {
            allocationEnds.await()
            Result.success(listOf(ApAllocationOutcome.NotAvailable))
        }
        val model = viewModel()
        val caller = launch { context.awaitOutcomes {} }
        runCurrent()
        model.onApproveClicked()
        runCurrent()

        caller.cancel()
        runCurrent()
        verifyPromptClosed(times = 0)

        allocationEnds.complete(Unit)
        runCurrent()

        verifyPromptClosed(times = 1)
    }

    @Test
    fun `the allocation prompt clears its own context when it goes away`() = runTest(testDispatcher) {
        holder.set(context)

        createAndClear { viewModel() }

        assertNull(holder.get())
    }

    @Test
    fun `a newer allocation prompt's context survives when an older one goes away`() = runTest(testDispatcher) {
        val newer = allocationContext("other.dot")
        holder.set(newer)

        createAndClear { viewModel() }

        assertSame(newer, holder.get())
    }

    private fun viewModel() = ResourceAllocationRequestViewModel(router, context, holder, interactor)

    private fun allocationContext(product: String) = ResourceAllocationRequestContext(
        productId = ProductId.fromStoredValue(product),
        resources = listOf(ApAllocatableResource.BulletInAllowance),
        onExisting = OnExistingAllowancePolicy.IGNORE,
    )

    // Closing goes by this prompt's own id, never a bare back() that would pop whichever sheet is on top
    private fun verifyPromptClosed(times: Int) {
        coVerify(exactly = times) { router.closeResourceAllocationRequestPrompt(context.id) }
        verify(exactly = 0) { router.back() }
    }
}
