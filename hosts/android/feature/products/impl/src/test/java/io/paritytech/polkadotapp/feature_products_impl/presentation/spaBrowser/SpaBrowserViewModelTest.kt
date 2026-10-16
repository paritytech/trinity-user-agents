package io.paritytech.polkadotapp.feature_products_impl.presentation.spaBrowser

import androidx.lifecycle.SavedStateHandle
import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsLoadProgress
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_products_api.domain.browser.ProductSessionController
import io.paritytech.polkadotapp.feature_products_api.domain.browser.TabInfo
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.derivation.ReservedProductIds
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Before
import org.junit.Test

private const val TAB_ID = 7L
private const val OTHER_TAB_ID = 8L

private val TLD = DotNsTld.parse("paseo")!!
private val PRODUCT = ProductId.fromStoredValue("getcash.paseo")

class SpaBrowserViewModelTest {
    private val testDispatcher = StandardTestDispatcher()

    private val productSessionController: ProductSessionController = mockk(relaxUnitFun = true)
    private val router: ProductsRouter = mockk(relaxUnitFun = true)
    private val dotNsTldProvider: DotNsTldProvider = mockk()
    private val openTabs = MutableStateFlow(listOf(activeTab()))

    @Before
    fun setUp() {
        Dispatchers.setMain(testDispatcher)
        every { productSessionController.activeTab } returns MutableStateFlow(null)
        every { productSessionController.openTabs } returns openTabs
        every { productSessionController.webView } returns MutableStateFlow(null)
        every { productSessionController.loadProgress } returns MutableStateFlow(DotNsLoadProgress.Idle)
        coEvery { dotNsTldProvider.getTld() } returns Result.success(TLD)
    }

    @After
    fun tearDown() = Dispatchers.resetMain()

    // A player in the middle of a round would lose it, so the new build waits for their consent. The prompt names
    // the product, since "the app" could as well be read as the host app.
    @Test
    fun `a new build is offered rather than loaded`() = runTest(testDispatcher) {
        withOutdatedTab()
        val model = viewModel()

        model.onScreenStarted()
        advanceUntilIdle()

        assertEquals(PRODUCT.value, model.state.value.newBuildPromptProduct)
        verifyTabNotReloaded()
    }

    // A built-in product should feel like part of the app, which updates without asking either.
    @Test
    fun `a built-in product is not asked about its new build`() = runTest(testDispatcher) {
        coEvery { productSessionController.productWithNewBuild(TAB_ID) } returns Result.success(ReservedProductIds.game(TLD))
        val model = viewModel()

        model.onScreenStarted()
        advanceUntilIdle()

        assertNull(model.state.value.newBuildPromptProduct)
        verifyTabNotReloaded()
    }

    @Test
    fun `agreeing to the new build reloads the tab`() = runTest(testDispatcher) {
        withOutdatedTab()
        val model = viewModel()
        model.onScreenStarted()
        advanceUntilIdle()

        model.onReloadForNewBuildConfirmed()
        advanceUntilIdle()

        assertNull(model.state.value.newBuildPromptProduct)
        verify { productSessionController.reloadTab(TAB_ID) }
    }

    @Test
    fun `declining the new build leaves the tab as it is`() = runTest(testDispatcher) {
        withOutdatedTab()
        val model = viewModel()
        model.onScreenStarted()
        advanceUntilIdle()

        model.onReloadForNewBuildDismissed()
        advanceUntilIdle()

        assertNull(model.state.value.newBuildPromptProduct)
        verifyTabNotReloaded()
    }

    @Test
    fun `a tab on the latest build is left alone`() = runTest(testDispatcher) {
        coEvery { productSessionController.productWithNewBuild(TAB_ID) } returns Result.success(null)
        val model = viewModel()

        model.onScreenStarted()
        advanceUntilIdle()

        assertNull(model.state.value.newBuildPromptProduct)
    }

    // Coming back to the app offline must not interrupt the product with an error.
    @Test
    fun `a check that cannot reach the chain is left unanswered`() = runTest(testDispatcher) {
        coEvery { productSessionController.productWithNewBuild(TAB_ID) } returns Result.failure(IllegalStateException("no node"))
        val model = viewModel()

        model.onScreenStarted()
        advanceUntilIdle()

        assertNull(model.state.value.newBuildPromptProduct)
    }

    // The answer comes from the chain, and by then the user may be looking at another product.
    @Test
    fun `a new build is not offered over a tab the user switched to`() = runTest(testDispatcher) {
        val answer = CompletableDeferred<Result<ProductId?>>()
        coEvery { productSessionController.productWithNewBuild(TAB_ID) } coAnswers { answer.await() }
        val model = viewModel()
        model.onScreenStarted()
        advanceUntilIdle()

        openTabs.value = listOf(activeTab().copy(isActive = false), activeTab().copy(id = OTHER_TAB_ID))
        answer.complete(Result.success(PRODUCT))
        advanceUntilIdle()

        assertNull(model.state.value.newBuildPromptProduct)
    }

    // A tab that woke up from sleep already runs the latest build, so an offer made before must not stay up.
    @Test
    fun `a later check that finds the tab up to date withdraws the offer`() = runTest(testDispatcher) {
        withOutdatedTab()
        val model = viewModel()
        model.onScreenStarted()
        advanceUntilIdle()

        coEvery { productSessionController.productWithNewBuild(TAB_ID) } returns Result.success(null)
        model.onScreenStarted()
        advanceUntilIdle()

        assertNull(model.state.value.newBuildPromptProduct)
    }

    private fun withOutdatedTab() {
        coEvery { productSessionController.productWithNewBuild(TAB_ID) } returns Result.success(PRODUCT)
    }

    private fun verifyTabNotReloaded() {
        verify(exactly = 0) { productSessionController.reloadTab(any()) }
    }

    private fun activeTab() = TabInfo(id = TAB_ID, title = "Game", host = "game.dot", iconUrl = null, isActive = true)

    private fun viewModel() = SpaBrowserViewModel(
        productSessionController = productSessionController,
        interactor = mockk(),
        router = router,
        dotNsTldProvider = dotNsTldProvider,
        savedStateHandle = SavedStateHandle(),
    )
}
