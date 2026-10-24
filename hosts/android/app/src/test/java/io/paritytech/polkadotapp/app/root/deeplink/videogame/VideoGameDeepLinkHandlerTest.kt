package io.paritytech.polkadotapp.app.root.deeplink.videogame

import android.net.Uri
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_products_api.domain.game.ProductGameReminder
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.derivation.ReservedProductIds
import io.paritytech.polkadotapp.feature_videogame_impl.VideoGameRouter
import io.paritytech.polkadotapp.feature_videogame_impl.data.notifications.GameReminderEvent
import io.paritytech.polkadotapp.feature_videogame_impl.data.notifications.GameReminderOpenSource
import io.paritytech.polkadotapp.feature_videogame_impl.data.notifications.GameReminderTelemetry
import io.paritytech.polkadotapp.feature_videogame_impl.deeplink.PRODUCT_GAME_STARTS_AT_PARAM
import io.paritytech.polkadotapp.test_shared.TestCoroutineDispatchers
import io.paritytech.polkadotapp.test_shared.any
import io.paritytech.polkadotapp.test_shared.whenever
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.runBlocking
import org.junit.Test
import org.mockito.Mockito.inOrder
import org.mockito.Mockito.mock
import org.mockito.Mockito.never
import org.mockito.Mockito.verify

class VideoGameDeepLinkHandlerTest {
    private val tld = requireNotNull(DotNsTld.parse("dot"))
    private val game = ReservedProductIds.game(tld)
    private val other = ProductId.fromStoredValue("acme.dot")

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
        handle(reminderLink(game, START))

        verifyReported(GameReminderEvent.Tapped(game, START))
        verify(router).openGameProduct(game)
        verifyReported(GameReminderEvent.ProductOpened(game, START, GameReminderOpenSource.ALARM))
        verify(reminder).cancel(game)
    }

    @Test
    fun `a tap drops the held reminder before waiting for the accounts`() = runBlocking<Unit> {
        handle(reminderLink(game, START))

        val order = inOrder(reminder, accountRepository, router)
        order.verify(reminder).cancel(game)
        order.verify(accountRepository).getWalletAccount()
        order.verify(router).openGameProduct(game)
    }

    @Test
    fun `a link naming another product is reported as tapped and opens nothing`() = runBlocking<Unit> {
        handle(reminderLink(other, START))

        verifyReported(GameReminderEvent.Tapped(other, START))
        verify(router, never()).openGameProduct(any())
        verify(telemetry, never()).report(GameReminderEvent.ProductOpened(other, START, GameReminderOpenSource.ALARM))
    }

    private suspend fun handle(link: Uri) = coroutineScope { with(ComputationalScope(this)) { handler.handle(link) } }

    private fun reminderLink(productId: ProductId, startsAtMillis: Long): Uri = mock(Uri::class.java).also {
        whenever(it.pathSegments).thenReturn(listOf("product", productId.value))
        whenever(it.getQueryParameter(PRODUCT_GAME_STARTS_AT_PARAM)).thenReturn(startsAtMillis.toString())
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
