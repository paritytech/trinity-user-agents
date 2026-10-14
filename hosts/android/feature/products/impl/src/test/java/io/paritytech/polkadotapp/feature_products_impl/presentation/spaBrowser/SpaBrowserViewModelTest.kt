package io.paritytech.polkadotapp.feature_products_impl.presentation.spaBrowser

import androidx.lifecycle.SavedStateHandle
import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsLoadProgress
import io.paritytech.polkadotapp.feature_products_api.domain.browser.ProductSessionController
import io.paritytech.polkadotapp.feature_products_api.domain.browser.TabInfo
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test

private const val TAB_ID = 7L

class SpaBrowserViewModelTest {
    private val testDispatcher = StandardTestDispatcher()

    private val productSessionController: ProductSessionController = mockk(relaxUnitFun = true)
    private val router: ProductsRouter = mockk(relaxUnitFun = true)

    @Before
    fun setUp() {
        Dispatchers.setMain(testDispatcher)
        every { productSessionController.activeTab } returns MutableStateFlow(null)
        every { productSessionController.openTabs } returns MutableStateFlow(listOf(activeTab()))
        every { productSessionController.webView } returns MutableStateFlow(null)
        every { productSessionController.loadProgress } returns MutableStateFlow(DotNsLoadProgress.Idle)
    }

    @After
    fun tearDown() = Dispatchers.resetMain()

    // A player in the middle of a round would lose it, so the new build waits for their consent.
    @Test
    fun `a new build is offered rather than loaded`() = runTest(testDispatcher) {
        withOutdatedTab()
        val model = viewModel()

        model.onScreenStarted()
        advanceUntilIdle()

        assertTrue(model.state.value.isNewBuildPromptShown)
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

        assertFalse(model.state.value.isNewBuildPromptShown)
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

        assertFalse(model.state.value.isNewBuildPromptShown)
        verifyTabNotReloaded()
    }

    @Test
    fun `a tab on the latest build is left alone`() = runTest(testDispatcher) {
        coEvery { productSessionController.isTabOutdated(TAB_ID) } returns Result.success(false)
        val model = viewModel()

        model.onScreenStarted()
        advanceUntilIdle()

        assertFalse(model.state.value.isNewBuildPromptShown)
    }

    // Coming back to the app offline must not interrupt the product with an error.
    @Test
    fun `a check that cannot reach the chain is left unanswered`() = runTest(testDispatcher) {
        coEvery { productSessionController.isTabOutdated(TAB_ID) } returns Result.failure(IllegalStateException("no node"))
        val model = viewModel()

        model.onScreenStarted()
        advanceUntilIdle()

        assertFalse(model.state.value.isNewBuildPromptShown)
    }

    private fun withOutdatedTab() {
        coEvery { productSessionController.isTabOutdated(TAB_ID) } returns Result.success(true)
    }

    private fun verifyTabNotReloaded() {
        verify(exactly = 0) { productSessionController.reloadTab(any()) }
    }

    private fun activeTab() = TabInfo(id = TAB_ID, title = "Game", host = "game.dot", iconUrl = null, isActive = true)

    private fun viewModel() = SpaBrowserViewModel(
        productSessionController = productSessionController,
        interactor = mockk(),
        router = router,
        dotNsTldProvider = mockk(),
        savedStateHandle = SavedStateHandle(),
    )
}
