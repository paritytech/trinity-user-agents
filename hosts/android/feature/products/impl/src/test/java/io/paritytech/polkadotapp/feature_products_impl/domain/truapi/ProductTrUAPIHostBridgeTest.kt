package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.parity.truapi.HostBridge
import io.parity.truapi.TrUAPIHostRuntime
import io.parity.truapi.TrUAPIProductExecution
import io.paritytech.polkadotapp.common.data.storage.preferences.encrypted.EncryptedPreferences
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_products_api.domain.game.ProductGameReminder
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.HostApiInteractor
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.navigation.NavigationPolicy
import io.paritytech.polkadotapp.feature_products_impl.presentation.spaHost.ExpandedCardFace
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import okhttp3.OkHttpClient
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.mockito.Mockito.mock
import org.mockito.stubbing.Answer
import uniffi.truapi.ExpandedCardFaceOutcome
import uniffi.truapi.HostRejection
import uniffi.truapi.ProductExecutionKind
import uniffi.truapi.WsBridgeEndpoint

class ProductTrUAPIHostBridgeTest {
    private val refusingCore = Answer<Any> { throw IllegalStateException("loopback port unavailable") }

    private val gameReminder = mockk<ProductGameReminder>(relaxUnitFun = true)
    private val game = ProductId.fromStoredValue("game.dot")

    private fun TestScope.bridge(
        scope: CoroutineScope = CoroutineScope(StandardTestDispatcher(testScheduler)),
        hostApiInteractor: HostApiInteractor = mock(HostApiInteractor::class.java),
    ) = ProductTrUAPIHostBridge(
        hostApiInteractor = hostApiInteractor,
        chainHttpClient = OkHttpClient(),
        encryptedPreferences = mock(EncryptedPreferences::class.java),
        confirmationLauncher = mock(TrUAPIConfirmationLauncher::class.java),
        appLifecycleObserver = mockk { every { subscribe() } returns emptyFlow() },
        dotNsTldProvider = mock(DotNsTldProvider::class.java),
        pocketCardStore = mockk { every { observeCards() } returns emptyFlow() },
        productGameReminder = gameReminder,
        scope = scope,
    )

    @Test
    fun `a core that refuses to open the execution fails the attach instead of throwing`() = runTest {
        val outcome = bridge().attach(
            runtime = mock(TrUAPIHostRuntime::class.java, refusingCore),
            productId = ProductId.fromStoredValue("game.dot"),
            chains = EMPTY_CHAINS,
            navigationPolicy = NavigationPolicy.DeeplinkNavigation(onDeeplinkNavigation = {}),
            kind = ProductExecutionKind.APP,
            onReadyToInject = {},
        )

        assertTrue(outcome.isFailure)
    }

    // The worker supervisor never holds the bridge; it stops a worker by cancelling the scope it
    // handed in. That cancel is what has to close the execution, or every card scrolled off screen
    // leaves its execution, loopback registration and chain sockets alive for the whole process.
    @Test
    fun `cancelling the owning scope closes the execution`() = runTest {
        val scope = CoroutineScope(StandardTestDispatcher(testScheduler))
        val execution = mockk<TrUAPIProductExecution>(relaxUnitFun = true) {
            every { startWsBridge(any()) } returns WsBridgeEndpoint(port = 1u, token = "token")
        }
        val runtime = mockk<TrUAPIHostRuntime> {
            every { openProductExecution(any(), any(), any(), any(), any()) } returns execution
        }
        val bridge = bridge(scope, hostApiInteractor = mockk { every { subscribeTheme() } returns emptyFlow() })

        bridge.attach(
            runtime = runtime,
            productId = ProductId.fromStoredValue("game.dot"),
            chains = EMPTY_CHAINS,
            navigationPolicy = NavigationPolicy.DeeplinkNavigation(onDeeplinkNavigation = {}),
            kind = ProductExecutionKind.WORKER,
            onReadyToInject = {},
        )
        scope.cancel()
        advanceUntilIdle()

        verify { execution.stopWsBridge() }
        verify { execution.close() }
    }

    private suspend fun TestScope.attachCapturingBridge(card: ExpandedCardFace?): HostBridge {
        var opened: HostBridge? = null
        val runtime = mock(TrUAPIHostRuntime::class.java, Answer<Any> { invocation ->
            opened = invocation.getArgument(0)
            throw IllegalStateException("captured")
        })
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

        coVerify {
            gameReminder.schedule(game, 1_000)
            gameReminder.cancel(game)
        }
    }

    @Test
    fun `the game bridge rejects a schedule the reminder fails`() = runTest {
        withScheduleOutcome(Result.failure(IllegalStateException("notifications are not allowed")))
        val gameBridge = bridge().gameBridge(game)

        val outcome = runCatching { gameBridge.scheduleReminder(1_000u) }

        assertEquals("notifications are not allowed", (outcome.exceptionOrNull() as? HostRejection.Rejected)?.reason)
    }

    private fun withScheduleOutcome(outcome: Result<Unit>) {
        coEvery { gameReminder.schedule(game, 1_000) } returns outcome
    }
}
