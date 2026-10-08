package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.parity.truapi.TrUAPIHostRuntime
import io.paritytech.polkadotapp.common.data.storage.preferences.encrypted.EncryptedPreferences
import io.paritytech.polkadotapp.common.presentation.AppLifecycleObserver
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_products_api.domain.game.ProductGameReminder
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.HostApiInteractor
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.navigation.NavigationPolicy
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.PocketCardStore
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runTest
import okhttp3.OkHttpClient
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.truapi.HostRejection
import uniffi.truapi.ProductExecutionKind

class ProductTrUAPIHostBridgeTest {
    private val gameReminder = mockk<ProductGameReminder>(relaxed = true)
    private val game = ProductId.fromStoredValue("game.dot")

    private fun TestScope.bridge() = ProductTrUAPIHostBridge(
        hostApiInteractor = mockk<HostApiInteractor>(relaxed = true),
        chainHttpClient = OkHttpClient(),
        encryptedPreferences = mockk<EncryptedPreferences>(relaxed = true),
        coreStorage = mockk(),
        confirmationLauncher = mockk<TrUAPIConfirmationLauncher>(relaxed = true),
        appLifecycleObserver = mockk<AppLifecycleObserver>(relaxed = true),
        dotNsTldProvider = mockk<DotNsTldProvider>(relaxed = true),
        pocketCardStore = mockk<PocketCardStore>(relaxed = true),
        productGameReminder = gameReminder,
        scope = CoroutineScope(StandardTestDispatcher(testScheduler)),
    )

    @Test
    fun `a core that refuses to open the execution fails the attach instead of throwing`() = runTest {
        val runtime = mockk<TrUAPIHostRuntime> {
            every { openProductExecution(any(), any(), any(), any(), any()) } throws IllegalStateException("loopback port unavailable")
        }
        val outcome = bridge().attach(
            runtime = runtime,
            productId = ProductId.fromStoredValue("game.dot"),
            chains = EMPTY_CHAINS,
            navigationPolicy = NavigationPolicy.DeeplinkNavigation(onDeeplinkNavigation = {}),
            kind = ProductExecutionKind.APP,
            onReadyToInject = {},
        )

        assertTrue(outcome.isFailure)
    }

    @Test
    fun `the game bridge schedules and cancels for the calling product`() = runTest {
        withScheduleOutcome(Result.success(Unit))
        val gameBridge = bridge().gameBridge(game)

        gameBridge.scheduleReminder(1_000u)
        gameBridge.cancelReminder()

        coVerify { gameReminder.schedule(game, 1_000) }
        coVerify { gameReminder.cancel(game) }
    }

    @Test
    fun `the game bridge rejects a schedule the reminder fails`() = runTest {
        withScheduleOutcome(Result.failure(IllegalStateException("notifications are not allowed")))
        val gameBridge = bridge().gameBridge(game)

        val outcome = runCatching { gameBridge.scheduleReminder(1_000u) }

        assertEquals("notifications are not allowed", (outcome.exceptionOrNull() as? HostRejection.Rejected)?.reason)
    }

    private suspend fun withScheduleOutcome(outcome: Result<Unit>) {
        coEvery { gameReminder.schedule(game, 1_000) } returns outcome
    }
}
