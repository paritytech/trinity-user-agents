package io.paritytech.polkadotapp.feature_products_impl.presentation.spaBrowser

import android.webkit.WebView
import androidx.lifecycle.SavedStateHandle
import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsLoadProgress
import io.paritytech.polkadotapp.feature_products_api.domain.browser.BrowserSessionInfo
import io.paritytech.polkadotapp.feature_products_api.domain.browser.ProductSessionController
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Before
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class SpaBrowserViewModelTest {
    private val testDispatcher = StandardTestDispatcher()

    private val webView: WebView = mockk(relaxUnitFun = true)
    private val productSessionController: ProductSessionController = mockk(relaxUnitFun = true)
    private val router: ProductsRouter = mockk(relaxUnitFun = true)

    @Before
    fun setUp() {
        Dispatchers.setMain(testDispatcher)
        every { webView.canGoBack() } returns true
        every { productSessionController.activeTab } returns MutableStateFlow<BrowserSessionInfo?>(null)
        every { productSessionController.openTabs } returns MutableStateFlow(emptyList())
        every { productSessionController.webView } returns MutableStateFlow<WebView?>(webView)
        every { productSessionController.loadProgress } returns MutableStateFlow(DotNsLoadProgress.Idle)
    }

    @After
    fun tearDown() {
        Dispatchers.resetMain()
    }

    // Back steps through the page history first, which in a game runs many screens deep.
    @Test
    fun `minimize leaves at once and keeps the tab open, even when the page can go back`() {
        val model = viewModel()

        model.onMinimizeClick()

        verify { router.leaveBrowser() }
        verify(exactly = 0) { webView.goBack() }
        verify(exactly = 0) { productSessionController.closeActiveTab() }
        verify(exactly = 0) { productSessionController.closeTab(any()) }
    }

    private fun viewModel() = SpaBrowserViewModel(
        productSessionController = productSessionController,
        interactor = mockk(),
        router = router,
        dotNsTldProvider = mockk(),
        savedStateHandle = SavedStateHandle(),
    )
}
