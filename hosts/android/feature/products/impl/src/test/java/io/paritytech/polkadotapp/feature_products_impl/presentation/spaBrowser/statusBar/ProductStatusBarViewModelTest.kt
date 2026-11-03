package io.paritytech.polkadotapp.feature_products_impl.presentation.spaBrowser.statusBar

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.coVerifyOrder
import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.mockk.verifyOrder
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_products_api.domain.browser.BrowserSessionInfo
import io.paritytech.polkadotapp.feature_products_api.domain.browser.ProductSessionController
import io.paritytech.polkadotapp.feature_products_api.domain.error.ProductResolutionError
import io.paritytech.polkadotapp.feature_products_impl.domain.spaBrowser.statusBar.ProductStatusBarInteractor
import io.paritytech.polkadotapp.feature_products_impl.presentation.productBotManagement.ProductsRouter
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.awaitCancellation
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Before
import org.junit.Test
import uniffi.truapi.ProductId

private const val COIN_FLIP_URL = "https://coinflip.paseo"
private const val COIN_FLIP_GAME_URL = "https://coinflip.paseo/game"
private const val CHESS_URL = "https://chess.paseo"
private const val PLAIN_URL = "https://example.com"

@OptIn(ExperimentalCoroutinesApi::class)
class ProductStatusBarViewModelTest {
    private val testDispatcher = StandardTestDispatcher()

    private val coinFlip = ProductId.parse("coinflip.paseo").getOrThrow()
    private val chess = ProductId.parse("chess.paseo").getOrThrow()
    private val room = ChatId.fromRawValue(byteArrayOf(1, 2, 3))

    private val activeTab = MutableStateFlow<BrowserSessionInfo?>(null)
    private val productSessionController: ProductSessionController = mockk(relaxUnitFun = true) {
        every { activeTab } returns this@ProductStatusBarViewModelTest.activeTab
    }
    private val interactor: ProductStatusBarInteractor = mockk(relaxUnitFun = true)
    private val router: ProductsRouter = mockk(relaxUnitFun = true)

    @Before
    fun setUp() {
        Dispatchers.setMain(testDispatcher)
        coEvery { interactor.productIdOrNull(COIN_FLIP_URL) } returns coinFlip
        coEvery { interactor.productIdOrNull(COIN_FLIP_GAME_URL) } returns coinFlip
        coEvery { interactor.productIdOrNull(CHESS_URL) } returns chess
        coEvery { interactor.productIdOrNull(PLAIN_URL) } returns null
    }

    @After
    fun tearDown() {
        Dispatchers.resetMain()
    }

    @Test
    fun `the title follows the active tab`() = runBlocking<Unit> {
        // Given
        withChat(coinFlip, served = false)
        withChat(chess, served = false)
        val model = viewModel()

        // When / Then
        openTab(COIN_FLIP_URL, title = "Coin Flip")
        advance()
        assertEquals("Coin Flip", model.state.value.title)

        openTab(CHESS_URL, title = "Chess")
        advance()
        assertEquals("Chess", model.state.value.title)

        openTab(CHESS_URL, title = "Chess, move 3")
        advance()
        assertEquals("Chess, move 3", model.state.value.title)

        activeTab.value = null
        advance()
        assertEquals(ProductStatusBarUiState(title = "", menu = ProductMenu.WebPage), model.state.value)
    }

    @Test
    fun `the title shows while the product is still being identified`() = runBlocking<Unit> {
        // Given
        coEvery { interactor.productIdOrNull(COIN_FLIP_URL) } coAnswers { awaitCancellation() }
        val model = viewModel()

        // When
        openTab(COIN_FLIP_URL, title = "Coin Flip")
        advance()

        // Then
        assertEquals(ProductStatusBarUiState(title = "Coin Flip", menu = ProductMenu.WebPage), model.state.value)
    }

    @Test
    fun `chat is offered only when the product's worker serves it`() = runBlocking<Unit> {
        // Given
        withChat(coinFlip, served = true)
        withChat(chess, served = false)
        val model = viewModel()

        // When / Then
        openTab(COIN_FLIP_URL, title = "Coin Flip")
        advance()
        assertEquals(ProductMenu.Product(coinFlip, ProductChat.Served), model.state.value.menu)

        openTab(CHESS_URL, title = "Chess")
        advance()
        assertEquals(ProductMenu.Product(chess, ProductChat.NotServed), model.state.value.menu)
    }

    @Test
    fun `an answer for the previous tab never lands on the current one`() = runBlocking<Unit> {
        // Given
        val coinFlipAnswer = CompletableDeferred<Result<Boolean>>()
        coEvery { interactor.includesChat(coinFlip) } coAnswers { coinFlipAnswer.await() }
        withChat(chess, served = false)
        val model = viewModel()
        openTab(COIN_FLIP_URL, title = "Coin Flip")
        advance()

        // When
        openTab(CHESS_URL, title = "Chess")
        advance()
        val menuBeforeTheLateAnswer = model.state.value.menu
        model.onShareClick()
        coinFlipAnswer.complete(Result.success(true))
        advance()

        // Then
        assertEquals(ProductMenu.Product(chess, ProductChat.NotServed), menuBeforeTheLateAnswer)
        assertEquals(ProductMenu.Product(chess, ProductChat.NotServed), model.state.value.menu)
        verify(exactly = 1) { interactor.share(chess) }
        verify(exactly = 0) { interactor.share(coinFlip) }
    }

    @Test
    fun `a new product never shows the previous product's menu while its chat is looked up`() = runBlocking<Unit> {
        // Given
        withChat(coinFlip, served = true)
        coEvery { interactor.includesChat(chess) } coAnswers { awaitCancellation() }
        val model = viewModel()
        openTab(COIN_FLIP_URL, title = "Coin Flip")
        advance()

        // When
        openTab(CHESS_URL, title = "Chess")
        advance()
        model.onShareClick()
        model.onProceedInChatClick()
        advance()

        // Then
        assertEquals(ProductMenu.Product(chess, ProductChat.Unresolved), model.state.value.menu)
        verify(exactly = 1) { interactor.share(chess) }
        verify(exactly = 0) { interactor.share(coinFlip) }
        coVerify(exactly = 0) { interactor.installChatAndAwaitRoomCreated(any()) }
    }

    @Test
    fun `moving between pages of one product keeps its menu`() = runBlocking<Unit> {
        // Given
        withChat(coinFlip, served = true)
        val model = viewModel()
        openTab(COIN_FLIP_URL, title = "Coin Flip")
        advance()

        // When
        openTab(COIN_FLIP_GAME_URL, title = "Coin Flip")
        advance()

        // Then
        assertEquals(ProductMenu.Product(coinFlip, ProductChat.Served), model.state.value.menu)
        coVerify(exactly = 1) { interactor.includesChat(coinFlip) }
    }

    @Test
    fun `a product that cannot be resolved is marked unresolved`() = runBlocking<Unit> {
        // Given
        coEvery { interactor.includesChat(coinFlip) } returns Result.failure(ProductResolutionError.Unknown)
        val model = viewModel()

        // When
        openTab(COIN_FLIP_URL, title = "Coin Flip")
        advance()

        // Then
        assertEquals(ProductMenu.Product(coinFlip, ProductChat.Unresolved), model.state.value.menu)
    }

    @Test
    fun `reload looks the product up again after a failed resolution`() = runBlocking<Unit> {
        // Given
        coEvery { interactor.includesChat(coinFlip) } returnsMany listOf(
            Result.failure(ProductResolutionError.Unknown),
            Result.success(true),
        )
        val model = viewModel()
        openTab(COIN_FLIP_URL, title = "Coin Flip")
        advance()

        // When
        model.onReloadClick()
        advance()

        // Then
        assertEquals(ProductMenu.Product(coinFlip, ProductChat.Served), model.state.value.menu)
    }

    @Test
    fun `a plain page offers neither share nor chat`() = runBlocking<Unit> {
        // Given
        val model = viewModel()

        // When
        openTab(PLAIN_URL, title = "Example")
        advance()
        model.onShareClick()
        model.onProceedInChatClick()
        advance()

        // Then
        assertEquals(ProductStatusBarUiState(title = "Example", menu = ProductMenu.WebPage), model.state.value)
        verify(exactly = 0) { interactor.share(any()) }
        coVerify(exactly = 0) { interactor.installChatAndAwaitRoomCreated(any()) }
    }

    @Test
    fun `share shares the active product`() = runBlocking<Unit> {
        // Given
        withChat(coinFlip, served = false)
        val model = viewModel()
        openTab(COIN_FLIP_URL, title = "Coin Flip")
        advance()

        // When
        model.onShareClick()

        // Then
        verify(exactly = 1) { interactor.share(coinFlip) }
    }

    @Test
    fun `reload reloads the active tab and stays in the browser`() = runBlocking<Unit> {
        // Given
        val model = viewModel()

        // When
        model.onReloadClick()

        // Then
        verify(exactly = 1) { productSessionController.reload() }
        verify(exactly = 0) { productSessionController.closeActiveTab() }
        verify(exactly = 0) { router.leaveBrowser() }
    }

    @Test
    fun `close app closes the tab and leaves the browser`() = runBlocking<Unit> {
        // Given
        val model = viewModel()

        // When
        model.onCloseAppClick()

        // Then
        verifyOrder {
            productSessionController.closeActiveTab()
            router.leaveBrowser()
        }
        verify(exactly = 1) { router.leaveBrowser() }
    }

    @Test
    fun `close app tapped twice acts once per tap`() = runBlocking<Unit> {
        // Given
        val model = viewModel()

        // When
        model.onCloseAppClick()
        model.onCloseAppClick()

        // Then
        verify(exactly = 2) { productSessionController.closeActiveTab() }
        verify(exactly = 2) { router.leaveBrowser() }
    }

    @Test
    fun `proceed in chat installs the chat and opens its room`() = runBlocking<Unit> {
        // Given
        withChat(coinFlip, served = true)
        coEvery { interactor.installChatAndAwaitRoomCreated(coinFlip) } returns Result.success(room)
        val model = viewModel()
        openTab(COIN_FLIP_URL, title = "Coin Flip")
        advance()

        // When
        model.onProceedInChatClick()
        advance()

        // Then
        coVerifyOrder {
            interactor.installChatAndAwaitRoomCreated(coinFlip)
            router.openChat(room)
        }
        verify(exactly = 0) { productSessionController.closeActiveTab() }
        verify(exactly = 0) { router.leaveBrowser() }
    }

    @Test
    fun `proceed in chat does nothing for a product whose worker serves no chat`() = runBlocking<Unit> {
        // Given
        withChat(coinFlip, served = false)
        val model = viewModel()
        openTab(COIN_FLIP_URL, title = "Coin Flip")
        advance()

        // When
        model.onProceedInChatClick()
        advance()

        // Then
        coVerify(exactly = 0) { interactor.installChatAndAwaitRoomCreated(any()) }
        verify(exactly = 0) { router.openChat(any()) }
    }

    @Test
    fun `a second proceed in chat while the room is pending does nothing`() = runBlocking<Unit> {
        // Given
        withChat(coinFlip, served = true)
        val pendingRoom = CompletableDeferred<Result<ChatId>>()
        coEvery { interactor.installChatAndAwaitRoomCreated(coinFlip) } coAnswers { pendingRoom.await() }
        val model = viewModel()
        openTab(COIN_FLIP_URL, title = "Coin Flip")
        advance()

        // When
        model.onProceedInChatClick()
        advance()
        model.onProceedInChatClick()
        pendingRoom.complete(Result.success(room))
        advance()

        // Then
        coVerify(exactly = 1) { interactor.installChatAndAwaitRoomCreated(coinFlip) }
        verify(exactly = 1) { router.openChat(room) }
    }

    @Test
    fun `proceed in chat works again once the chat has opened`() = runBlocking<Unit> {
        // Given
        withChat(coinFlip, served = true)
        coEvery { interactor.installChatAndAwaitRoomCreated(coinFlip) } returns Result.success(room)
        val model = viewModel()
        openTab(COIN_FLIP_URL, title = "Coin Flip")
        advance()

        // When
        model.onProceedInChatClick()
        advance()
        model.onProceedInChatClick()
        advance()

        // Then
        coVerify(exactly = 2) { interactor.installChatAndAwaitRoomCreated(coinFlip) }
        verify(exactly = 2) { router.openChat(room) }
    }

    @Test
    fun `a failed chat install opens nothing and can be retried`() = runBlocking<Unit> {
        // Given
        withChat(coinFlip, served = true)
        coEvery { interactor.installChatAndAwaitRoomCreated(coinFlip) } returns Result.failure(IllegalStateException())
        val model = viewModel()
        openTab(COIN_FLIP_URL, title = "Coin Flip")
        advance()

        // When
        model.onProceedInChatClick()
        advance()
        model.onProceedInChatClick()
        advance()

        // Then
        coVerify(exactly = 2) { interactor.installChatAndAwaitRoomCreated(coinFlip) }
        verify(exactly = 0) { router.openChat(any()) }
    }

    private fun advance() = testDispatcher.scheduler.advanceUntilIdle()

    private fun viewModel() = ProductStatusBarViewModel(
        productSessionController = productSessionController,
        interactor = interactor,
        router = router,
    )

    private fun withChat(productId: ProductId, served: Boolean) {
        coEvery { interactor.includesChat(productId) } returns Result.success(served)
    }

    private fun openTab(url: String, title: String) {
        activeTab.value = BrowserSessionInfo(
            title = title,
            host = null,
            url = url,
            isLoading = false,
            loadFraction = null,
            isDevServer = false,
        )
    }
}
