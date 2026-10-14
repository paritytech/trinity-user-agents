package io.paritytech.polkadotapp.feature_videogame_impl.presentation.autoLaunch

import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.common.data.app.AppLifecycleState
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.presentation.AppLifecycleObserver
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_videogame_impl.VideoGameRouter
import io.paritytech.polkadotapp.feature_videogame_impl.domain.notifications.RealProductGameReminder
import io.paritytech.polkadotapp.feature_videogame_impl.domain.notifications.ScheduledProductGame
import io.paritytech.polkadotapp.test_shared.FakeTimeProvider
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class ProductGameAutoOpenerTest {
    private val reminder: RealProductGameReminder = mockk(relaxUnitFun = true)
    private val router: VideoGameRouter = mockk(relaxUnitFun = true)
    private val lifecycle: AppLifecycleObserver = mockk()
    private val lifecycleState = MutableStateFlow(AppLifecycleState.FOREGROUND)

    private val game = ProductId.fromStoredValue("game.dot")
    private val other = ProductId.fromStoredValue("acme.dot")
    private val scheduledGame = ScheduledProductGame(game.value, startsAtMillis = 60_000)
    private val scheduledOther = ScheduledProductGame(other.value, startsAtMillis = 120_000)

    @Test
    fun `opens the soonest product at its start while in the foreground and clears it`() = runTest {
        startOpener(scheduledOther, scheduledGame)

        advanceTimeBy(59_999)
        runCurrent()
        coVerify(exactly = 0) { router.openGameProduct(game) }

        advanceTimeBy(1)
        runCurrent()
        coVerify(exactly = 1) { router.openGameProduct(game) }
        coVerify(exactly = 1) { reminder.clear(scheduledGame) }
        coVerify(exactly = 0) { router.openGameProduct(other) }
    }

    @Test
    fun `opens a game whose start passed within the grace`() = runTest {
        advanceTimeBy(scheduledGame.startsAtMillis + 29_999)

        startOpener(scheduledGame)

        coVerify(exactly = 1) { router.openGameProduct(game) }
    }

    @Test
    fun `does not open a game whose start passed beyond the grace`() = runTest {
        advanceTimeBy(scheduledGame.startsAtMillis + 30_000)

        startOpener(scheduledGame)

        coVerify(exactly = 0) { router.openGameProduct(game) }
    }

    @Test
    fun `leaving the foreground mid-countdown neither opens nor clears the game`() = runTest {
        startOpener(scheduledGame)

        advanceTimeBy(30_000)
        lifecycleState.value = AppLifecycleState.BACKGROUND
        runCurrent()
        advanceTimeBy(31_000)
        runCurrent()

        coVerify(exactly = 0) { router.openGameProduct(game) }
        coVerify(exactly = 0) { reminder.clear(scheduledGame) }
    }

    private fun TestScope.startOpener(vararg games: ScheduledProductGame) {
        every { reminder.scheduled } returns MutableStateFlow(games.toList())
        every { lifecycle.subscribe() } returns lifecycleState
        val opener = ProductGameAutoOpener(reminder, lifecycle, router, FakeTimeProvider { testScheduler.currentTime })

        with(ComputationalScope(backgroundScope)) { opener.initialize() }
        runCurrent()
    }
}
