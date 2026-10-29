package io.paritytech.polkadotapp.app.root.deeplink.videogame

import android.net.Uri
import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.coVerifyOrder
import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_products_api.domain.game.ProductGameReminder
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.derivation.ReservedProductIds
import io.paritytech.polkadotapp.feature_videogame_impl.VideoGameRouter
import io.paritytech.polkadotapp.feature_videogame_impl.domain.notifications.GameReminderEvent
import io.paritytech.polkadotapp.feature_videogame_impl.domain.notifications.GameReminderTelemetry
import io.paritytech.polkadotapp.test_shared.TestCoroutineDispatchers
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertTrue
import org.junit.Test

class VideoGameDeepLinkHandlerTest {
    private val tld = requireNotNull(DotNsTld.parse("dot"))
    private val game = ReservedProductIds.game(tld)
    private val other = ProductId.fromStoredValue("acme.dot")

    private val router: VideoGameRouter = mockk(relaxUnitFun = true)
    private val reminder: ProductGameReminder = mockk(relaxUnitFun = true)
    private val telemetry: GameReminderTelemetry = mockk(relaxUnitFun = true)
    private val accountRepository: AccountRepository = mockk {
        coEvery { getWalletAccount() } returns mockk()
    }

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
        coVerify(exactly = 1) { router.openGameProduct(game) }
        verifyReported(GameReminderEvent.ProductOpened(game, START))
        coVerify(exactly = 1) { reminder.cancel(game) }
    }

    @Test
    fun `a valid reminder link opens before cancelling its reminder`() = runBlocking<Unit> {
        withScheduledGame()

        handle(reminderLink(game))

        coVerifyOrder {
            reminder.scheduledStartFor(game)
            telemetry.report(GameReminderEvent.Tapped(game, START))
            accountRepository.getWalletAccount()
            router.openGameProduct(game)
            reminder.cancel(game)
            telemetry.report(GameReminderEvent.ProductOpened(game, START))
        }
    }

    @Test
    fun `a link naming another product reports nothing and opens nothing`() = runBlocking<Unit> {
        handle(reminderLink(other))

        verify(exactly = 0) { telemetry.report(any()) }
        coVerify(exactly = 0) { reminder.cancel(any()) }
        coVerify(exactly = 0) { accountRepository.getWalletAccount() }
        coVerify(exactly = 0) { router.openGameProduct(any()) }
    }

    @Test
    fun `a failed open leaves the reminder held and reports no opened event`() = runBlocking<Unit> {
        withScheduledGame()
        coEvery { router.openGameProduct(game) } throws IllegalStateException("failed to open")

        val result = handle(reminderLink(game))

        assertTrue(result.isFailure)
        coVerify(exactly = 0) { reminder.cancel(game) }
        verify(exactly = 0) { telemetry.report(GameReminderEvent.ProductOpened(game, START)) }
    }

    private suspend fun handle(link: Uri) = coroutineScope { with(ComputationalScope(this)) { handler.handle(link) } }

    private fun reminderLink(productId: ProductId): Uri = mockk {
        every { pathSegments } returns listOf("product", productId.value)
    }

    private fun withScheduledGame() {
        every { reminder.scheduledStartFor(game) } returns START
    }

    private fun verifyReported(event: GameReminderEvent) {
        verify(exactly = 1) { telemetry.report(event) }
    }

    private class FixedTldProvider(private val tld: DotNsTld) : DotNsTldProvider {
        override fun currentTldOrNull(): DotNsTld = tld

        override suspend fun getTld(): Result<DotNsTld> = Result.success(tld)
    }

    private companion object {
        const val START = 1_000_000_060_000L
    }
}
