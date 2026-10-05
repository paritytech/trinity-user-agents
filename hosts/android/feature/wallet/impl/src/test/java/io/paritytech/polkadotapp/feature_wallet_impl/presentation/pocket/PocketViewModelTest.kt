package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket

import android.content.Context
import android.webkit.WebView
import androidx.lifecycle.viewModelScope
import io.paritytech.polkadotapp.common.presentation.sharing.SharingManager
import io.paritytech.polkadotapp.feature_coinage_api.domain.model.BackupProgress
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsLoadProgress
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCard
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardId
import io.paritytech.polkadotapp.feature_products_api.domain.pocket.PocketCardKey
import io.paritytech.polkadotapp.feature_products_api.model.JsWidget
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.presentation.spaHost.SpaHost
import io.paritytech.polkadotapp.feature_products_api.presentation.spaHost.SpaHostSession
import io.paritytech.polkadotapp.feature_tokens_api.presentation.formatter.TokenAmountFormatter
import io.paritytech.polkadotapp.feature_tokens_api.presentation.mapper.TokenAmountMapper
import io.paritytech.polkadotapp.feature_videogame_api.domain.collectibles.CollectiblesUrlResolver
import io.paritytech.polkadotapp.feature_wallet_impl.PocketRouter
import io.paritytech.polkadotapp.feature_wallet_impl.domain.interactor.PocketInteractor
import io.paritytech.polkadotapp.feature_wallet_impl.domain.model.PocketRank
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.PocketCardUiModel
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.models.PocketScreenState
import io.paritytech.polkadotapp.test_shared.TestCoroutineDispatchers
import io.paritytech.polkadotapp.test_shared.any
import io.paritytech.polkadotapp.test_shared.whenever
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.mockito.Mockito.mock
import org.mockito.Mockito.never
import org.mockito.Mockito.verify
import org.mockito.stubbing.Answer

class PocketViewModelTest {
    // Every flow the screen combines answers empty unless a test says otherwise, so each test names
    // only the source it is about. observeRank is answered here rather than stubbed because its
    // context parameter cannot be named from a call site that does not have one.
    private val quietFlows = Answer { invocation ->
        when {
            invocation.method.name == "observeRank" -> flowOf(PocketRank.Basic)
            invocation.method.name == "warmUpProduct" -> Result.success(Unit)
            invocation.method.returnType == Flow::class.java -> emptyFlow<Any>()
            else -> null
        }
    }

    private val interactor: PocketInteractor = mock(PocketInteractor::class.java, quietFlows)

    // SpaHost.createSession carries context parameters, which cannot be named from a call site that
    // has none, so the mock answers it by name.
    private val hostedSession = FakeSpaHostSession()
    private val spaHost: SpaHost = mock(SpaHost::class.java, Answer { hostedSession })

    // One dispatcher for the screen's own scope and for the pool its card list is assembled on, so
    // the whole pipeline runs on this test's clock. Given a real background dispatcher the list
    // would be assembled on threads the test cannot advance, leaving it to wait on wall-clock time
    // and to lose that wait on a loaded machine.
    private val testDispatcher = StandardTestDispatcher()
    private val dispatchers = TestCoroutineDispatchers(testDispatcher)

    private class FakeSpaHostSession : SpaHostSession {
        override val webView = MutableStateFlow<WebView?>(null)
        override val currentUrl = MutableStateFlow("")
        override val loadProgress = MutableStateFlow<DotNsLoadProgress>(DotNsLoadProgress.Idle)
        override val title = MutableStateFlow("")
        override fun pauseConnections() = Unit
        override fun resumeConnections() = Unit
    }

    // The card list is shared eagerly in the view model's own scope, which outlives the test unless
    // it is cancelled.
    private val created = mutableListOf<PocketViewModel>()

    private fun createViewModel() = PocketViewModel(
        interactor = interactor,
        tokenAmountMapper = mock(TokenAmountMapper::class.java),
        tokenAmountFormatter = mock(TokenAmountFormatter::class.java),
        router = mock(PocketRouter::class.java),
        collectiblesUrlResolver = mock(CollectiblesUrlResolver::class.java),
        idShareImageRenderer = mock(IdShareImageRenderer::class.java),
        sharingManager = mock(SharingManager::class.java),
        dispatchers = dispatchers,
        spaHost = spaHost,
        context = mock(Context::class.java),
    ).also { created += it }

    /** The cards the screen holds once everything the view model started has run. */
    private fun TestScope.settledCards(viewModel: PocketViewModel): List<PocketCardUiModel> {
        advanceUntilIdle()

        return viewModel.cards.value
    }

    @Before
    fun setUp() {
        Dispatchers.setMain(testDispatcher)
        whenever(interactor.observeUsername()).thenReturn(flowOf("alicent"))
        whenever(interactor.observeAddress()).thenReturn(flowOf("15oF4u"))
        whenever(interactor.observeBackupProgress()).thenReturn(flowOf(BackupProgress.Unknown))
        whenever(interactor.observeAccountBackupPending()).thenReturn(flowOf(false))
    }

    @After
    fun tearDown() {
        created.forEach { it.viewModelScope.cancel() }
        Dispatchers.resetMain()
    }

    private fun productCard(cardId: String) = PocketCard(
        key = PocketCardKey(ProductId.fromStoredValue("game.dot"), PocketCardId(cardId)),
        title = cardId,
        privileged = false,
    )

    private fun privilegedCard(cardId: String) = PocketCard(
        key = PocketCardKey(ProductId.fromStoredValue("peopl.dot"), PocketCardId(cardId)),
        title = cardId,
        privileged = true,
    )

    // The collection is stored, decoded and served by a product's worker, so it has failure modes
    // the balance and identity cards do not share. Before the product cards joined this screen
    // nothing product-side could empty it; that must stay true.
    @Test
    fun `a failing product collection costs the product cards alone, not the native ones`() = runTest(testDispatcher) {
        whenever(interactor.observeProductCards()).thenReturn(flow { throw IllegalStateException("unreadable") })

        val cards = settledCards(createViewModel())

        assertEquals(listOf("digital_dollar_card", "id_card"), cards.map { it.id })
    }

    @Test
    fun `product cards follow the native ones once the collection loads`() = runTest(testDispatcher) {
        whenever(interactor.observeProductCards()).thenReturn(flowOf(listOf(productCard("loyalty"))))

        val cards = settledCards(createViewModel())

        assertEquals(
            listOf("digital_dollar_card", "id_card", "product_card:game.dot:loyalty"),
            cards.map { it.id },
        )
        assertEquals("loyalty", cards.filterIsInstance<PocketCardUiModel.ProductCard>().single().title)
    }

    // A product can remove its own card, and the collection can become unreadable, while the card
    // is open. The screen falls back to the list on its own, but the card stays selected unless
    // something clears it, so the card coming back pops the details view open with no tap — and the
    // product hosted under it was never taken down. Releasing that product is
    // ExpandedProductPageTest's half of this.
    @Test
    fun `a card that leaves the collection is no longer the selected one`() = runTest(testDispatcher) {
        val collection = MutableStateFlow(listOf(productCard("loyalty")))
        whenever(interactor.observeProductCards()).thenReturn(collection)

        val viewModel = createViewModel()
        val card = settledCards(viewModel).filterIsInstance<PocketCardUiModel.ProductCard>().single()
        viewModel.selectCard(card)
        advanceUntilIdle()
        assertTrue(viewModel.state.value is PocketScreenState.CardDetails)

        collection.value = emptyList()
        advanceUntilIdle()
        assertTrue(viewModel.state.value is PocketScreenState.List)

        collection.value = listOf(productCard("loyalty"))
        advanceUntilIdle()

        assertTrue("the card came back selected", viewModel.state.value is PocketScreenState.List)
        assertNull(viewModel.expandedProductSession.value)
    }

    /**
     * A host-placed card is the one most likely to be opened, and only the host places one, so the
     * set cannot grow with use. Fetching its pages when the collection arrives spends the time the
     * user spends looking at the cards; waiting for [PocketViewModel.selectCard] offers only the
     * half second the card takes to travel.
     */
    @Test
    fun `a privileged card's product is fetched as soon as the collection arrives`() = runTest(testDispatcher) {
        val card = privilegedCard("humanity")
        whenever(interactor.observeProductCards()).thenReturn(flowOf(listOf(card)))

        val viewModel = createViewModel()
        settledCards(viewModel)

        verify(interactor).warmUpProduct(card.key)
    }

    // The cost of warming every card is what the privileged set exists to avoid: a user holding
    // cards from a dozen products would otherwise fetch a dozen archives on opening the tab.
    @Test
    fun `a card the user added is fetched only once it is selected`() = runTest(testDispatcher) {
        val card = productCard("loyalty")
        whenever(interactor.observeProductCards()).thenReturn(flowOf(listOf(card)))

        val viewModel = createViewModel()
        val uiCard = settledCards(viewModel).filterIsInstance<PocketCardUiModel.ProductCard>().single()
        verify(interactor, never()).warmUpProduct(any())

        viewModel.selectCard(uiCard)
        advanceUntilIdle()

        verify(interactor).warmUpProduct(card.key)
    }

    // The collection re-emits whenever it changes, and a fetch answered from cache still costs a
    // chain read to reach that cache.
    @Test
    fun `a privileged product is fetched once however often the collection changes`() = runTest(testDispatcher) {
        val privileged = privilegedCard("humanity")
        val collection = MutableStateFlow(listOf(privileged))
        whenever(interactor.observeProductCards()).thenReturn(collection)

        val viewModel = createViewModel()
        settledCards(viewModel)

        collection.value = listOf(privileged, productCard("loyalty"))
        advanceUntilIdle()

        verify(interactor).warmUpProduct(privileged.key)
    }

    // Expanding a card draws a second copy of it, and that copy's first frame shows whatever face it
    // can read the moment it is composed. A face the list copy already holds but the new copy has to
    // wait for draws the expanded card empty for the frames the wait takes.
    @Test
    fun `a card drawn again can read the face the first copy already holds`() = runTest(testDispatcher) {
        val card = productCard("loyalty")
        val face = JsWidget.Text(text = "Loyalty")
        whenever(interactor.observeProductCards()).thenReturn(flowOf(listOf(card)))
        whenever(interactor.observeFace(card.key)).thenReturn(flowOf(face))

        val viewModel = createViewModel()
        val uiCard = settledCards(viewModel).filterIsInstance<PocketCardUiModel.ProductCard>().single()
        val listCopy = launch { viewModel.bindingsOf(uiCard).face.collect {} }
        advanceUntilIdle()

        assertEquals(face, viewModel.bindingsOf(uiCard).face.value)
        listCopy.cancel()
    }
}
