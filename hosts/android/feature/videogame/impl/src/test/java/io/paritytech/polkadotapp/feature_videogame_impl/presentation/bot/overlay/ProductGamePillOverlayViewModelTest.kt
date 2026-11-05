package io.paritytech.polkadotapp.feature_videogame_impl.presentation.bot.overlay

import io.paritytech.polkadotapp.feature_videogame_impl.VideoGameRouter
import io.paritytech.polkadotapp.feature_videogame_impl.domain.notifications.RealProductGameReminder
import io.paritytech.polkadotapp.feature_videogame_impl.domain.notifications.ScheduledProductGame
import io.paritytech.polkadotapp.test_shared.whenever
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.setMain
import kotlinx.coroutines.withTimeout
import org.junit.After
import org.junit.Before
import org.junit.Test
import org.mockito.Mockito.mock
import org.mockito.Mockito.never
import org.mockito.Mockito.timeout
import org.mockito.Mockito.verify
import uniffi.truapi.ProductId

@OptIn(ExperimentalCoroutinesApi::class)
class ProductGamePillOverlayViewModelTest {
    private val reminder: RealProductGameReminder = mock()
    private val router: VideoGameRouter = mock()

    private val game = ProductId.parse("dim2.paseo").getOrThrow()
    private val humanity = ProductId.parse("peopl.paseo").getOrThrow()

    @Before
    fun setUp() {
        Dispatchers.setMain(Dispatchers.Unconfined)
    }

    @After
    fun tearDown() {
        Dispatchers.resetMain()
    }

    @Test
    fun `a click on the game's pill opens the game product`() = runBlocking<Unit> {
        clickPillFor(game)

        verify(router, timeout(TIMEOUT_MS)).openGameProduct(game)
    }

    // Humanity books the reminder once it runs the game in its Polkadot room
    // (individuality-apps#1281); the game is played in the game product, so that is what opens.
    @Test
    fun `a click on Humanity's pill opens the game product`() = runBlocking<Unit> {
        clickPillFor(humanity)

        verify(router, timeout(TIMEOUT_MS)).openGameProduct(game)
        verify(router, never()).openGameProduct(humanity)
    }

    private suspend fun clickPillFor(bookedBy: ProductId) {
        val startsAt = System.currentTimeMillis() + 60_000
        whenever(reminder.scheduled).thenReturn(MutableStateFlow(listOf(ScheduledProductGame(bookedBy, startsAt))))
        val viewModel = ProductGamePillOverlayViewModel(reminder, router)

        withTimeout(TIMEOUT_MS) { viewModel.pillState.first { it is VideoGamePillState.Shown } }
        viewModel.onPillClicked()
    }

    private companion object {
        const val TIMEOUT_MS = 5_000L
    }
}
