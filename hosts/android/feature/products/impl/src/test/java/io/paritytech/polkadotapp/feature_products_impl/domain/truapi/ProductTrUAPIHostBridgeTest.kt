package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.parity.truapi.HostBridge
import io.parity.truapi.TrUAPIHostRuntime
import io.paritytech.polkadotapp.common.data.storage.preferences.encrypted.EncryptedPreferences
import io.paritytech.polkadotapp.common.presentation.AppLifecycleObserver
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_products_api.domain.game.ProductGameReminder
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.HostApiInteractor
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.navigation.NavigationPolicy
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.PocketCardStore
import io.paritytech.polkadotapp.feature_products_impl.presentation.spaHost.ExpandedCardFace
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runTest
import okhttp3.OkHttpClient
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.truapi.ExpandedCardFaceOutcome
import uniffi.truapi.HostRejection
import uniffi.truapi.ProductExecutionKind

class ProductTrUAPIHostBridgeTest {
    private fun refusingCore(): TrUAPIHostRuntime = mockk {
        every { openProductExecution(any(), any(), any(), any(), any()) } throws IllegalStateException("loopback port unavailable")
    }

    private val gameReminder = mockk<ProductGameReminder>(relaxUnitFun = true)
    private val game = ProductId.fromStoredValue("game.dot")

    private fun TestScope.bridge() = ProductTrUAPIHostBridge(
        hostApiInteractor = mockk<HostApiInteractor>(),
        chainHttpClient = OkHttpClient(),
        encryptedPreferences = mockk<EncryptedPreferences>(),
        confirmationLauncher = mockk<TrUAPIConfirmationLauncher>(),
        appLifecycleObserver = mockk<AppLifecycleObserver>(),
        dotNsTldProvider = mockk<DotNsTldProvider>(),
        pocketCardStore = mockk<PocketCardStore>(),
        productGameReminder = gameReminder,
        scope = CoroutineScope(StandardTestDispatcher(testScheduler)),
    )

    @Test
    fun `a core that refuses to open the execution fails the attach instead of throwing`() = runTest {
        val outcome = bridge().attach(
            runtime = refusingCore(),
            productId = ProductId.fromStoredValue("game.dot"),
            chains = EMPTY_CHAINS,
            navigationPolicy = NavigationPolicy.DeeplinkNavigation(onDeeplinkNavigation = {}),
            kind = ProductExecutionKind.APP,
            onReadyToInject = {},
        )

        assertTrue(outcome.isFailure)
    }

    private suspend fun TestScope.attachCapturingBridge(card: ExpandedCardFace?): HostBridge {
        var opened: HostBridge? = null
        val runtime = mockk<TrUAPIHostRuntime> {
            every { openProductExecution(any(), any(), any(), any(), any()) } answers {
                opened = firstArg()
                throw IllegalStateException("captured")
            }
        }
        bridge().attach(
            runtime = runtime,
            productId = game,
            chains = EMPTY_CHAINS,
            navigationPolicy = NavigationPolicy.DeeplinkNavigation(onDeeplinkNavigation = {}),
            kind = ProductExecutionKind.WIDGET,
            card = card,
            onReadyToInject = {},
        )
        return checkNotNull(opened)
    }

    // Only a session drawn under a card has a face to move; any other session must say so rather
    // than pretend.
    @Test
    fun `a product that is not under a card cannot move a face`() = runTest {
        val hostBridge = attachCapturingBridge(card = null)

        assertEquals(ExpandedCardFaceOutcome.UNSUPPORTED, hostBridge.setExpandedCardFaceShown(true))
    }

    @Test
    fun `a product under a card asks that card and gets its answer`() = runTest {
        val asked = mutableListOf<Boolean>()
        val hostBridge = attachCapturingBridge(ExpandedCardFace { shown ->
            asked += shown
            ExpandedCardFaceOutcome.USER_MOVING
        })

        val outcome = hostBridge.setExpandedCardFaceShown(false)

        assertEquals(listOf(false), asked)
        assertEquals(ExpandedCardFaceOutcome.USER_MOVING, outcome)
    }

    @Test
    fun `the game bridge schedules and cancels for the calling product`() = runTest {
        withScheduleOutcome(Result.success(Unit))
        val gameBridge = bridge().gameBridge(game)

        gameBridge.scheduleReminder(1_000u)
        gameBridge.cancelReminder()

        coVerify(exactly = 1) { gameReminder.schedule(game, 1_000) }
        coVerify(exactly = 1) { gameReminder.cancel(game) }
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
