package io.paritytech.polkadotapp.feature_products_impl.presentation.spaBrowser

import android.webkit.WebView
import androidx.lifecycle.SavedStateHandle
import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsLoadProgress
import io.paritytech.polkadotapp.feature_products_api.domain.browser.BrowserSessionInfo
import io.paritytech.polkadotapp.feature_products_api.domain.browser.ProductSessionController
import io.paritytech.polkadotapp.feature_products_api.domain.browser.TabInfo
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class SpaBrowserViewModelTest {
    private val testDispatcher = StandardTestDispatcher()

    private val webView: WebView = mockk(relaxUnitFun = true)
    private val openTabs = MutableStateFlow(emptyList<TabInfo>())
    private val productSessionController: ProductSessionController = mockk(relaxUnitFun = true)
    private val router: ProductsRouter = mockk(relaxUnitFun = true)

    @Before
    fun setUp() {
        Dispatchers.setMain(testDispatcher)
        every { productSessionController.activeTab } returns MutableStateFlow<BrowserSessionInfo?>(null)
        every { productSessionController.openTabs } returns openTabs
        every { productSessionController.webView } returns MutableStateFlow<WebView?>(webView)
        every { productSessionController.loadProgress } returns MutableStateFlow(DotNsLoadProgress.Idle)
    }

    @After
    fun tearDown() {
        Dispatchers.resetMain()
    }

    @Test
    fun `back walks the page history before leaving`() {
        // Given
        every { webView.canGoBack() } returns true
        val model = viewModel()

        // When
        model.onBackPressed()

        // Then
        verify { webView.goBack() }
        verify(exactly = 0) { router.leaveBrowser() }
    }

    @Test
    fun `back with nothing left to unwind leaves and keeps the tab open`() {
        // Given
        every { webView.canGoBack() } returns false
        every { productSessionController.goToPreviousTab() } returns false
        val model = viewModel()

        // When
        model.onBackPressed()

        // Then
        verify(exactly = 1) { router.leaveBrowser() }
        verify(exactly = 0) { productSessionController.closeActiveTab() }
        verify(exactly = 0) { productSessionController.closeTab(any()) }
    }

    @Test
    fun `close app leaves the browser once whether or not other tabs remain`() {
        // Given
        every { productSessionController.closeActiveTab() } answers {
            openTabs.value = openTabs.value.filterNot { it.isActive }.map { it.copy(isActive = true) }
        }
        var leaves = 0
        every { router.leaveBrowser() } answers { leaves++ }
        openTabs.value = listOf(tab(id = 1, active = true), tab(id = 2, active = false))
        val withOtherTabs = viewModel()
        advance()

        // When
        withOtherTabs.onCloseAppClick()
        advance()
        val leavesWithOtherTabs = leaves
        openTabs.value = listOf(tab(id = 3, active = true))
        val onTheLastTab = viewModel()
        advance()
        onTheLastTab.onCloseAppClick()
        advance()

        // Then
        assertEquals(listOf(1, 2), listOf(leavesWithOtherTabs, leaves))
    }

    private fun advance() = testDispatcher.scheduler.advanceUntilIdle()

    private fun viewModel() = SpaBrowserViewModel(
        productSessionController = productSessionController,
        interactor = mockk(),
        productUpdater = mockk(),
        router = router,
        savedStateHandle = SavedStateHandle(),
    )

    private fun tab(id: Long, active: Boolean) = TabInfo(id = id, title = "", host = null, iconUrl = null, isActive = active)
}
