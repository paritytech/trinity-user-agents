package io.paritytech.polkadotapp.feature_products_impl.presentation.spaBrowser

import android.net.Uri
import android.webkit.WebView
import androidx.lifecycle.SavedStateHandle
import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.coVerifyOrder
import io.mockk.every
import io.mockk.mockk
import io.mockk.mockkStatic
import io.mockk.unmockkStatic
import io.mockk.verify
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsLoadProgress
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_products_api.domain.browser.BrowserSessionInfo
import io.paritytech.polkadotapp.feature_products_api.domain.browser.ProductSessionController
import io.paritytech.polkadotapp.feature_products_api.model.ExecutableHost
import io.paritytech.polkadotapp.feature_products_api.model.Executables
import io.paritytech.polkadotapp.feature_products_api.model.Product
import io.paritytech.polkadotapp.feature_products_api.model.ProductExecutable
import io.paritytech.polkadotapp.feature_products_api.model.ResolvedProduct
import io.paritytech.polkadotapp.feature_products_api.model.SemVer
import io.paritytech.polkadotapp.feature_products_api.model.derivation.ReservedProductIds
import io.paritytech.polkadotapp.feature_products_impl.domain.product.update.ProductUpdater
import io.paritytech.polkadotapp.feature_products_impl.domain.spaBrowser.SpaBrowserInteractor
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
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
import uniffi.truapi.ProductId

private val TLD = DotNsTld.parse("paseo")!!
private val PRODUCT = ProductId.parse("getcash.paseo").getOrThrow()
private val OTHER_PRODUCT = ProductId.parse("swap.paseo").getOrThrow()

private const val PRODUCT_APP = "app.getcash.paseo"
private const val PRODUCT_WORKER = "worker.getcash.paseo"
private const val OTHER_PRODUCT_APP = "app.swap.paseo"

class SpaBrowserViewModelTest {
    private val testDispatcher = StandardTestDispatcher()

    private val activeTab = MutableStateFlow<BrowserSessionInfo?>(null)
    private val pendingNames = MutableStateFlow<Set<String>>(emptySet())

    private val webView: WebView = mockk(relaxUnitFun = true)
    private val productSessionController: ProductSessionController = mockk(relaxUnitFun = true)
    private val productUpdater: ProductUpdater = mockk(relaxUnitFun = true)
    private val interactor: SpaBrowserInteractor = mockk()
    private val router: ProductsRouter = mockk(relaxUnitFun = true)
    private val dotNsTldProvider: DotNsTldProvider = mockk()

    @Before
    fun setUp() {
        Dispatchers.setMain(testDispatcher)
        mockkStatic(Uri::class)
        activeTab.value = tabOn(PRODUCT)
        every { productSessionController.activeTab } returns activeTab
        every { productSessionController.openTabs } returns MutableStateFlow(emptyList())
        every { productSessionController.webView } returns MutableStateFlow<WebView?>(webView)
        every { productSessionController.loadProgress } returns MutableStateFlow(DotNsLoadProgress.Idle)
        every { productUpdater.observePendingNames() } returns pendingNames
        coEvery { productUpdater.check(any()) } returns Result.success(Unit)
        coEvery { interactor.resolveProduct(any()) } answers { Result.success(resolved(firstArg())) }
        coEvery { dotNsTldProvider.getTld() } returns Result.success(TLD)
    }

    @After
    fun tearDown() {
        unmockkStatic(Uri::class)
        Dispatchers.resetMain()
    }

    @Test
    fun `a downloaded build is offered rather than loaded`() = runTest(testDispatcher) {
        val model = viewModel()

        pendingNames.value = setOf(PRODUCT_APP)
        advanceUntilIdle()

        assertEquals("Name of getcash.paseo", model.state.value.updateReadyProductName)
        verifyNothingApplied()
    }

    @Test
    fun `a build downloaded for another product is not offered`() = runTest(testDispatcher) {
        val model = viewModel()

        pendingNames.value = setOf(OTHER_PRODUCT_APP)
        advanceUntilIdle()

        assertNull(model.state.value.updateReadyProductName)
    }

    @Test
    fun `a new build of the product's worker alone is not offered on its page`() = runTest(testDispatcher) {
        val model = viewModel()

        pendingNames.value = setOf(PRODUCT_WORKER)
        advanceUntilIdle()

        assertNull(model.state.value.updateReadyProductName)
    }

    @Test
    fun `a built-in product is not asked about its new build`() = runTest(testDispatcher) {
        val game = ReservedProductIds.game(TLD)
        activeTab.value = tabOn(game)
        val model = viewModel()

        pendingNames.value = setOf("app.${game.value}")
        advanceUntilIdle()

        assertNull(model.state.value.updateReadyProductName)
    }

    @Test
    fun `agreeing to the new build switches only the page and reloads it`() = runTest(testDispatcher) {
        val model = viewModel()
        pendingNames.value = setOf(PRODUCT_APP, PRODUCT_WORKER)
        advanceUntilIdle()

        model.onUpdateReloadClick()
        advanceUntilIdle()

        coVerifyOrder {
            productUpdater.applyPage(PRODUCT, PRODUCT_APP)
            productSessionController.reload()
        }
        coVerify(exactly = 0) { productUpdater.apply(any()) }
    }

    @Test
    fun `refreshing switches only the page before it reloads`() = runTest(testDispatcher) {
        val model = viewModel()
        pendingNames.value = setOf(PRODUCT_WORKER)
        advanceUntilIdle()

        model.onRefresh()
        advanceUntilIdle()

        coVerifyOrder {
            productUpdater.applyPage(PRODUCT, PRODUCT_APP)
            productSessionController.reload()
        }
        coVerify(exactly = 0) { productUpdater.apply(any()) }
    }

    @Test
    fun `refreshing a page that failed to load still reloads it`() = runTest(testDispatcher) {
        coEvery { interactor.resolveProduct(any()) } returns Result.failure(IllegalStateException("no node"))
        val model = viewModel()
        advanceUntilIdle()

        model.onRefresh()
        advanceUntilIdle()

        verify { productSessionController.reload() }
    }

    @Test
    fun `putting the new build off hides the offer until the screen is next started`() = runTest(testDispatcher) {
        val model = viewModel()
        pendingNames.value = setOf(PRODUCT_APP)
        advanceUntilIdle()

        model.onUpdateDismissClick()
        advanceUntilIdle()
        assertNull(model.state.value.updateReadyProductName)
        verifyNothingApplied()

        model.onScreenStarted()
        advanceUntilIdle()
        assertEquals("Name of getcash.paseo", model.state.value.updateReadyProductName)
    }

    @Test
    fun `starting the screen checks the product on it for a new build`() = runTest(testDispatcher) {
        val model = viewModel()

        model.onScreenStarted()
        advanceUntilIdle()

        coVerify { productUpdater.check(PRODUCT) }
    }

    @Test
    fun `a check that cannot reach the chain is left unanswered`() = runTest(testDispatcher) {
        coEvery { productUpdater.check(PRODUCT) } returns Result.failure(IllegalStateException("no node"))
        val model = viewModel()

        model.onScreenStarted()
        advanceUntilIdle()

        assertNull(model.state.value.updateReadyProductName)
    }

    // Back steps through the page history first, which in a game runs many screens deep.
    @Test
    fun `minimize leaves at once and keeps the tab open, even when the page can go back`() {
        // Given
        every { webView.canGoBack() } returns true
        val model = viewModel()

        // When
        model.onMinimizeClick()

        // Then
        verify { router.leaveBrowser() }
        verify(exactly = 0) { webView.goBack() }
        verify(exactly = 0) { productSessionController.closeActiveTab() }
        verify(exactly = 0) { productSessionController.closeTab(any()) }
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

    private fun verifyNothingApplied() {
        coVerify(exactly = 0) { productUpdater.applyPage(any(), any()) }
        verify(exactly = 0) { productSessionController.reload() }
    }

    private fun resolved(productId: ProductId) = ResolvedProduct(
        product = Product(id = productId, name = "Name of ${productId.value}", icon = null),
        description = null,
        executables = Executables(
            app = ProductExecutable.App(host = ExecutableHost("app.${productId.value}"), appVersion = SemVer.ZERO),
            widget = null,
            worker = null,
        ),
    )

    private fun tabOn(productId: ProductId): BrowserSessionInfo {
        val url = "https://${productId.value}/"
        stubUri(url, host = productId.value)

        return BrowserSessionInfo(
            title = productId.value,
            host = productId.value,
            url = url,
            isLoading = false,
            loadFraction = null,
            isDevServer = false,
        )
    }

    private fun stubUri(url: String, host: String) {
        val uri = mockk<Uri>()
        val builder = mockk<Uri.Builder>()
        every { uri.toString() } returns url
        every { uri.host } returns host
        every { uri.buildUpon() } returns builder
        every { builder.authority(any()) } returns builder
        every { builder.build() } returns uri
        every { Uri.parse(url) } returns uri
    }

    private fun viewModel() = SpaBrowserViewModel(
        productSessionController = productSessionController,
        interactor = interactor,
        productUpdater = productUpdater,
        router = router,
        dotNsTldProvider = dotNsTldProvider,
        savedStateHandle = SavedStateHandle(),
    )
}
