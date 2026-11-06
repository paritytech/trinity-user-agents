package io.paritytech.polkadotapp.feature_products_impl.presentation.spaBrowser.statusBar

import androidx.lifecycle.SavedStateHandle
import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.coVerifyOrder
import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.feature_products_api.domain.browser.BrowserSessionInfo
import io.paritytech.polkadotapp.feature_products_api.domain.browser.ProductSessionController
import io.paritytech.polkadotapp.feature_products_api.domain.browser.TabInfo
import io.paritytech.polkadotapp.feature_products_api.domain.error.ProductResolutionError
import io.paritytech.polkadotapp.feature_products_api.presentation.SpaBrowserPayload
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
        assertEquals("Coin Flip", model.state.value?.title)

        openTab(CHESS_URL, title = "Chess")
        advance()
        assertEquals("Chess", model.state.value?.title)

        openTab(CHESS_URL, title = "Chess, move 3")
        advance()
        assertEquals("Chess, move 3", model.state.value?.title)

        activeTab.value = null
        advance()
        assertEquals(ProductStatusBarUiState(title = "", menu = ProductMenu.WebPage), model.state.value)
    }

    @Test
    fun `the previous tab's title is never shown for a newly opened product`() = runBlocking<Unit> {
        // Given
        withChat(chess, served = false)
        openTab(COIN_FLIP_URL, title = "Coin Flip")
        val model = viewModel(openedFor = SpaBrowserPayload.ByProductId(chess))
        advance()
        val beforeTheTabOpens = model.state.value

        // When
        openTab(CHESS_URL, title = "Chess")
        advance()

        // Then
        assertEquals(null, beforeTheTabOpens)
        assertEquals("Chess", model.state.value?.title)
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
        assertEquals(ProductMenu.Product(coinFlip, ProductChat.Served), model.state.value?.menu)

        openTab(CHESS_URL, title = "Chess")
        advance()
        assertEquals(ProductMenu.Product(chess, ProductChat.NotServed), model.state.value?.menu)
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
        val menuBeforeTheLateAnswer = model.state.value?.menu
        coinFlipAnswer.complete(Result.success(true))
        advance()

        // Then
        assertEquals(ProductMenu.Product(chess, ProductChat.NotServed), menuBeforeTheLateAnswer)
        assertEquals(ProductMenu.Product(chess, ProductChat.NotServed), model.state.value?.menu)
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
        assertEquals(ProductMenu.Product(chess, ProductChat.Unresolved), model.state.value?.menu)
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
        assertEquals(ProductMenu.Product(coinFlip, ProductChat.Served), model.state.value?.menu)
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
        assertEquals(ProductMenu.Product(coinFlip, ProductChat.Unresolved), model.state.value?.menu)
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
        assertEquals(ProductMenu.Product(coinFlip, ProductChat.Served), model.state.value?.menu)
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

    @Test
    fun `close app leaves the browser itself unless it closes the only tab`() {
        // Given
        val openTabs = MutableStateFlow(listOf(tab(id = 1, active = true), tab(id = 2, active = false)))
        every { productSessionController.openTabs } returns openTabs
        every { productSessionController.closeActiveTab() } answers {
            val remaining = openTabs.value.filterNot { it.isActive }
            openTabs.value = if (remaining.size < openTabs.value.size) remaining.map { it.copy(isActive = true) } else remaining
        }
        var tap = 0
        val tapsThatLeft = mutableListOf<Int>()
        every { router.leaveBrowser() } answers { tapsThatLeft += tap }
        val model = viewModel()

        // When
        tap = 1
        model.onCloseAppClick()
        tap = 2
        model.onCloseAppClick()
        openTabs.value = listOf(tab(id = 3, active = false))
        tap = 3
        model.onCloseAppClick()

        // Then
        assertEquals(listOf(1, 3), tapsThatLeft)
    }

    private fun advance() = testDispatcher.scheduler.advanceUntilIdle()

    private fun viewModel(openedFor: SpaBrowserPayload? = null) = ProductStatusBarViewModel(
        savedStateHandle = SavedStateHandle(mapOf(SpaBrowserPayload::class.java.name to openedFor)),
        productSessionController = productSessionController,
        interactor = interactor,
        router = router,
    )

    private fun tab(id: Long, active: Boolean) = TabInfo(id = id, title = "", host = null, iconUrl = null, isActive = active)

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
