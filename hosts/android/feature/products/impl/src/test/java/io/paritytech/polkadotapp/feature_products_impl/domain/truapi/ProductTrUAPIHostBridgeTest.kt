package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import android.content.Context
import uniffi.truapi.AccountAccessReview
import uniffi.truapi.PermissionDecision
import uniffi.truapi.ProfileDisclosureReview
import uniffi.truapi.UserConfirmationReview
import dagger.Lazy
import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.mockk
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.PermissionAuthorizationChanges
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.ProductPermissionRepository
import io.paritytech.polkadotapp.feature_settings_api.domain.language.AppLanguageProvider
import kotlinx.coroutines.flow.flowOf
import io.mockk.every
import io.mockk.mockkObject
import io.mockk.unmockkObject
import io.mockk.verify
import io.parity.truapi.HostBridge
import android.content.IntentFilter
import androidx.core.content.ContextCompat
import io.parity.truapi.LocalhostBridgeBootstrap
import io.parity.truapi.TrUAPIHostRuntime
import io.parity.truapi.TrUAPIProductExecution
import io.paritytech.polkadotapp.common.data.storage.preferences.encrypted.EncryptedPreferences
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_products_api.domain.game.ProductGameReminder
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.HostApiInteractor
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.navigation.NavigationPolicy
import io.paritytech.polkadotapp.feature_products_impl.presentation.spaHost.ExpandedCardFace
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.async
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import kotlinx.coroutines.test.resetMain
import okhttp3.OkHttpClient
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.fail
import org.junit.Assert.assertTrue
import org.junit.Test
import org.mockito.Mockito.mock
import org.mockito.Mockito.mockConstruction
import org.mockito.Mockito.mockStatic
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
        launcher: TrUAPIConfirmationLauncher = mock(TrUAPIConfirmationLauncher::class.java),
        scope: CoroutineScope = CoroutineScope(StandardTestDispatcher(testScheduler)),
        hostApiInteractor: HostApiInteractor = mock(HostApiInteractor::class.java),
    ) = ProductTrUAPIHostBridge(
        hostApiInteractor = hostApiInteractor,
        chainHttpClient = OkHttpClient(),
        encryptedPreferences = mock(EncryptedPreferences::class.java),
        confirmationLauncher = launcher,
        appLifecycleObserver = mockk { every { subscribe() } returns emptyFlow() },
        dotNsTldProvider = mock(DotNsTldProvider::class.java),
        pocketCardStore = mockk { every { observeCards() } returns emptyFlow() },
        context = mock(Context::class.java),
        appLanguageProvider = object : AppLanguageProvider {
            override val languageTag = flowOf("en-US")
        },
        permissionRepository = Lazy { mockk { coEvery { getAllByProduct(any()) } returns emptyList() } },
        permissionChanges = PermissionAuthorizationChanges(),
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
            onPermissionRevoked = {},
            onReadyToInject = {},
        )

        assertTrue(outcome.isFailure)
    }

    private suspend fun TestScope.callbacks(launcher: TrUAPIConfirmationLauncher): HostBridge {
        var callbacks: HostBridge? = null
        val runtime = mock(TrUAPIHostRuntime::class.java, Answer<Any> { invocation ->
            callbacks = invocation.arguments.filterIsInstance<HostBridge>().single()
            throw IllegalStateException("captured callbacks without opening a native execution")
        })
        val outcome = bridge(launcher).attach(
            runtime = runtime,
            productId = ProductId.fromStoredValue("game.dot"),
            chains = EMPTY_CHAINS,
            navigationPolicy = NavigationPolicy.DeeplinkNavigation(onDeeplinkNavigation = {}),
            kind = ProductExecutionKind.APP,
            onPermissionRevoked = {},
            onReadyToInject = {},
        )
        assertTrue(outcome.isFailure)
        return checkNotNull(callbacks)
    }

    @Test
    fun `profile permission and action callbacks await explicit review without optimistic approval`() = runTest {
        val review = UserConfirmationReview.ProfileDisclosure(ProfileDisclosureReview("seity.paseo"))
        for (permissionCallback in listOf(true, false)) {
            for (approved in listOf(true, false)) {
                val launcher = mockk<TrUAPIConfirmationLauncher>()
                val prompted = CompletableDeferred<TrUAPIConfirmation>()
                val answer = CompletableDeferred<Boolean>()
                coEvery { launcher.awaitDecision(any()) } coAnswers {
                    prompted.complete(firstArg())
                    answer.await()
                }
                val callbacks = callbacks(launcher)
                val pending = async {
                    if (permissionCallback) callbacks.confirmPermission(review) else callbacks.confirmUserAction(review)
                }

                val confirmation = prompted.await() as TrUAPIConfirmation.ProfileDisclosure
                assertEquals("seity.paseo", confirmation.requesterProductId)
                assertFalse(pending.isCompleted)
                answer.complete(approved)
                val expected: Any = if (permissionCallback) {
                    if (approved) PermissionDecision.ALLOW_ALWAYS else PermissionDecision.DENY
                } else {
                    approved
                }
                assertEquals(expected, pending.await())
                coVerify(exactly = 1) { launcher.awaitDecision(any()) }
            }
        }
    }

    @Test
    fun `profile prompt failure propagates rather than becoming a durable permission decision`() = runTest {
        val launcher = mockk<TrUAPIConfirmationLauncher>()
        coEvery { launcher.awaitDecision(any()) } throws HostRejection.Rejected("review unavailable")
        val callbacks = callbacks(launcher)
        val review = UserConfirmationReview.ProfileDisclosure(ProfileDisclosureReview("seity.paseo"))

        try {
            callbacks.confirmPermission(review)
            fail("A failed prompt must not return a permission decision")
        } catch (failure: HostRejection.Rejected) {
            assertEquals("review unavailable", failure.reason)
        }
        coVerify(exactly = 1) { launcher.awaitDecision(any()) }
    }

    @Test
    fun `supported permission preserves explicit approval and denial through product callbacks`() = runTest {
        val review = UserConfirmationReview.AccountAccess(AccountAccessReview("game.dot", "target.dot"))
        for (approved in listOf(true, false)) {
            val prompts = mutableListOf<TrUAPIConfirmation>()
            val launcher = mockk<TrUAPIConfirmationLauncher>()
            coEvery { launcher.awaitDecision(capture(prompts)) } returns approved
            val callbacks = callbacks(launcher)

            assertEquals(
                if (approved) PermissionDecision.ALLOW_ALWAYS else PermissionDecision.DENY,
                callbacks.confirmPermission(review),
            )
            val prompt = prompts.single() as TrUAPIConfirmation.AccountAccess
            assertEquals("game.dot", prompt.requesterProductId)
            assertEquals("target.dot", prompt.targetProductId)
        }
    }

    // The worker supervisor stops a worker by cancelling the scope it handed in.
    // That must also release its execution, loopback registration and chain sockets.
    @Test
    fun `cancelling the owning scope closes the execution`() = runTest {
        val dispatcher = StandardTestDispatcher(testScheduler)
        val scope = CoroutineScope(dispatcher)
        Dispatchers.setMain(dispatcher)
        mockkObject(LocalhostBridgeBootstrap)
        try {
            every { LocalhostBridgeBootstrap.script(any(), any()) } returns "bootstrap"
            mockStatic(ContextCompat::class.java).use {
                mockConstruction(IntentFilter::class.java).use {
                    val execution = mockk<TrUAPIProductExecution>(relaxUnitFun = true) {
                        every { startWsBridge(any()) } returns WsBridgeEndpoint(port = 1u, token = "token")
                    }
                    val runtime = mockk<TrUAPIHostRuntime> {
                        every { openProductExecution(any(), any(), any(), any(), any()) } returns execution
                    }
                    val bridge = bridge(
                        scope = scope,
                        hostApiInteractor = mockk { every { subscribeTheme() } returns emptyFlow() },
                    )

                    val outcome = bridge.attach(
                        runtime = runtime,
                        productId = game,
                        chains = EMPTY_CHAINS,
                        navigationPolicy = NavigationPolicy.DeeplinkNavigation(onDeeplinkNavigation = {}),
                        kind = ProductExecutionKind.WORKER,
                        onPermissionRevoked = {},
                        onReadyToInject = {},
                    )
                    assertTrue(outcome.isSuccess)
                    scope.cancel()
                    advanceUntilIdle()

                    verify(exactly = 1) { execution.stopWsBridge() }
                    verify(exactly = 1) { execution.close() }
                }
            }
        } finally {
            scope.cancel()
            unmockkObject(LocalhostBridgeBootstrap)
            Dispatchers.resetMain()
        }
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
            onPermissionRevoked = {},
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
