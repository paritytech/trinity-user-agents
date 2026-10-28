package io.paritytech.polkadotapp.feature_products_impl.presentation.spaBrowser

import android.net.Uri
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
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
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

private val TLD = DotNsTld.parse("paseo")!!
private val PRODUCT = ProductId.fromStoredValue("getcash.paseo")
private val OTHER_PRODUCT = ProductId.fromStoredValue("swap.paseo")

private const val PRODUCT_APP = "app.getcash.paseo"
private const val PRODUCT_WORKER = "worker.getcash.paseo"
private const val OTHER_PRODUCT_APP = "app.swap.paseo"

class SpaBrowserViewModelTest {
    private val testDispatcher = StandardTestDispatcher()

    private val activeTab = MutableStateFlow<BrowserSessionInfo?>(null)
    private val pendingNames = MutableStateFlow<Set<String>>(emptySet())

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
        every { productSessionController.webView } returns MutableStateFlow(null)
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

    // A player in the middle of a round would lose it, so the new build waits for their consent.
    @Test
    fun `a downloaded build is offered rather than loaded`() = runTest(testDispatcher) {
        val model = viewModel()

        pendingNames.value = setOf(PRODUCT_APP)
        advanceUntilIdle()

        assertEquals(PRODUCT, model.state.value.updateReadyFor)
        verifyNothingApplied()
    }

    @Test
    fun `a build downloaded for another product is not offered`() = runTest(testDispatcher) {
        val model = viewModel()

        pendingNames.value = setOf(OTHER_PRODUCT_APP)
        advanceUntilIdle()

        assertNull(model.state.value.updateReadyFor)
    }

    // Reloading the page would change nothing on screen; the worker picks its build up once the product is off screen.
    @Test
    fun `a new build of the product's worker alone is not offered on its page`() = runTest(testDispatcher) {
        val model = viewModel()

        pendingNames.value = setOf(PRODUCT_WORKER)
        advanceUntilIdle()

        assertNull(model.state.value.updateReadyFor)
    }

    // A built-in product should feel like part of the app, which updates without asking either.
    @Test
    fun `a built-in product is not asked about its new build`() = runTest(testDispatcher) {
        val game = ReservedProductIds.game(TLD)
        activeTab.value = tabOn(game)
        val model = viewModel()

        pendingNames.value = setOf("app.${game.value}")
        advanceUntilIdle()

        assertNull(model.state.value.updateReadyFor)
    }

    // The whole version, so the page does not come back against the manifest and worker of the one it replaces.
    @Test
    fun `agreeing to the new build applies the product's version and reloads the page`() = runTest(testDispatcher) {
        val model = viewModel()
        pendingNames.value = setOf(PRODUCT_APP, PRODUCT_WORKER)
        advanceUntilIdle()

        model.onUpdateReloadClick()
        advanceUntilIdle()

        coVerifyOrder {
            productUpdater.apply(PRODUCT)
            productSessionController.reload()
        }
    }

    // Refresh is the user asking for the latest; reloading the old build while a new one sits on the disk would not be.
    @Test
    fun `refreshing applies a downloaded version before the page reloads`() = runTest(testDispatcher) {
        val model = viewModel()
        pendingNames.value = setOf(PRODUCT_WORKER)
        advanceUntilIdle()

        model.onRefresh()
        advanceUntilIdle()

        coVerifyOrder {
            productUpdater.apply(PRODUCT)
            productSessionController.reload()
        }
    }

    // "Not now" means not on this visit; the build is still there to offer when the user comes back.
    @Test
    fun `putting the new build off hides the offer until the screen is next started`() = runTest(testDispatcher) {
        val model = viewModel()
        pendingNames.value = setOf(PRODUCT_APP)
        advanceUntilIdle()

        model.onUpdateDismissClick()
        advanceUntilIdle()
        assertNull(model.state.value.updateReadyFor)
        verifyNothingApplied()

        model.onScreenStarted()
        advanceUntilIdle()
        assertEquals(PRODUCT, model.state.value.updateReadyFor)
    }

    // The app may have been away for days; the product on screen is checked without waiting for the next background run.
    @Test
    fun `starting the screen checks the product on it for a new build`() = runTest(testDispatcher) {
        val model = viewModel()

        model.onScreenStarted()
        advanceUntilIdle()

        coVerify { productUpdater.check(PRODUCT) }
    }

    // Coming back to the app offline must not interrupt the product with an error.
    @Test
    fun `a check that cannot reach the chain is left unanswered`() = runTest(testDispatcher) {
        coEvery { productUpdater.check(PRODUCT) } returns Result.failure(IllegalStateException("no node"))
        val model = viewModel()

        model.onScreenStarted()
        advanceUntilIdle()

        assertNull(model.state.value.updateReadyFor)
    }

    private fun verifyNothingApplied() {
        coVerify(exactly = 0) { productUpdater.apply(any()) }
        verify(exactly = 0) { productSessionController.reload() }
    }

    private fun resolved(productId: ProductId) = ResolvedProduct(
        product = Product(id = productId, name = productId.value, icon = null),
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

    // The product id is read off the tab's url through android.net.Uri, which a unit test has to stand in for.
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
