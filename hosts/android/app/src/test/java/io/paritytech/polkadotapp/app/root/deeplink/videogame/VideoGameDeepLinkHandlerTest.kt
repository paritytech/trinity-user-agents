package io.paritytech.polkadotapp.app.root.deeplink.videogame

import android.net.Uri
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_products_api.domain.game.ProductGameReminder
import io.paritytech.polkadotapp.feature_products_api.model.derivation.ReservedProductIds
import io.paritytech.polkadotapp.feature_videogame_impl.VideoGameRouter
import io.paritytech.polkadotapp.feature_videogame_impl.domain.notifications.GameReminderEvent
import io.paritytech.polkadotapp.feature_videogame_impl.domain.notifications.GameReminderTelemetry
import io.paritytech.polkadotapp.test_shared.TestCoroutineDispatchers
import io.paritytech.polkadotapp.test_shared.any
import io.paritytech.polkadotapp.test_shared.whenever
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertTrue
import org.junit.Test
import org.mockito.Mockito.doThrow
import org.mockito.Mockito.inOrder
import org.mockito.Mockito.mock
import org.mockito.Mockito.never
import org.mockito.Mockito.verify
import uniffi.truapi.ProductId

class VideoGameDeepLinkHandlerTest {
    private val tld = requireNotNull(DotNsTld.parse("dot"))
    private val game = ReservedProductIds.game(tld)
    private val other = ProductId.parse("acme.dot").getOrThrow()

    private val router: VideoGameRouter = mock(VideoGameRouter::class.java)
    private val reminder: ProductGameReminder = mock(ProductGameReminder::class.java)
    private val telemetry: GameReminderTelemetry = mock(GameReminderTelemetry::class.java)
    private val accountRepository: AccountRepository = mock(AccountRepository::class.java)

    private val handler = VideoGameDeepLinkHandler(
        coroutineDispatchers = TestCoroutineDispatchers(Dispatchers.Unconfined),
        accountRepository = accountRepository,
        videoGameRouter = router,
        productGameReminder = reminder,
        dotNsTldProvider = FixedTldProvider(tld),
        telemetry = telemetry,
    )

    @Test
    fun `a tap on the reminder is reported, then the game product's open from the alarm`() = runBlocking<Unit> {
        withScheduledGame()

        handle(reminderLink(game))

        verifyReported(GameReminderEvent.Tapped(game, START))
        verify(router).openGameProduct(game)
        verifyReported(GameReminderEvent.ProductOpened(game, START))
        verify(reminder).cancel(game)
    }

    @Test
    fun `a valid reminder link opens before cancelling its reminder`() = runBlocking<Unit> {
        withScheduledGame()

        handle(reminderLink(game))

        val order = inOrder(reminder, telemetry, accountRepository, router)
        order.verify(reminder).scheduledStartFor(game)
        order.verify(telemetry).report(GameReminderEvent.Tapped(game, START))
        order.verify(accountRepository).getWalletAccount()
        order.verify(router).openGameProduct(game)
        order.verify(reminder).cancel(game)
        order.verify(telemetry).report(GameReminderEvent.ProductOpened(game, START))
    }

    @Test
    fun `a link naming another product reports nothing and opens nothing`() = runBlocking<Unit> {
        handle(reminderLink(other))

        verify(telemetry, never()).report(any())
        verify(reminder, never()).cancel(any())
        verify(accountRepository, never()).getWalletAccount()
        verify(router, never()).openGameProduct(any())
    }

    @Test
    fun `a failed open leaves the reminder held and reports no opened event`() = runBlocking<Unit> {
        withScheduledGame()
        doThrow(IllegalStateException("failed to open")).`when`(router).openGameProduct(game)

        val result = handle(reminderLink(game))

        assertTrue(result.isFailure)
        verify(reminder, never()).cancel(game)
        verify(telemetry, never()).report(GameReminderEvent.ProductOpened(game, START))
    }

    private suspend fun handle(link: Uri) = coroutineScope { with(ComputationalScope(this)) { handler.handle(link) } }

    private fun reminderLink(productId: ProductId): Uri = mock(Uri::class.java).also {
        whenever(it.pathSegments).thenReturn(listOf("product", productId.value))
    }

    private fun withScheduledGame() {
        whenever(reminder.scheduledStartFor(game)).thenReturn(START)
    }

    private fun verifyReported(event: GameReminderEvent) {
        verify(telemetry).report(event)
    }

    private class FixedTldProvider(private val tld: DotNsTld) : DotNsTldProvider {
        override fun currentTldOrNull(): DotNsTld = tld

        override suspend fun getTld(): Result<DotNsTld> = Result.success(tld)
    }

    private companion object {
        const val START = 1_000_000_060_000L
    }
}
