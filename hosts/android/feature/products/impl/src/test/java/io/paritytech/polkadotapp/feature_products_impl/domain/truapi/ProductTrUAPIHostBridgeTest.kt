package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import uniffi.truapi.AccountAccessReview
import uniffi.truapi.PermissionDecision
import uniffi.truapi.ProfileDisclosureReview
import uniffi.truapi.UserConfirmationReview
import dagger.Lazy
import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.mockk
import io.mockk.every
import io.paritytech.polkadotapp.tools_media_connection_impl.nativeMedia.NativeMediaBackendFactory
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.PermissionAuthorizationChanges
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.ProductPermissionRepository
import io.paritytech.polkadotapp.feature_settings_api.domain.language.AppLanguageProvider
import kotlinx.coroutines.flow.flowOf
import io.parity.truapi.HostBridge
import io.paritytech.polkadotapp.feature_products_api.domain.game.ProductGameReminder
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.navigation.NavigationPolicy
import io.paritytech.polkadotapp.feature_products_impl.presentation.spaHost.ExpandedCardFace
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.async
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runTest
import okhttp3.OkHttpClient
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.fail
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.truapi.ExpandedCardFaceOutcome
import uniffi.truapi.HostRejection
import uniffi.truapi.ProductExecutionKind

class ProductTrUAPIHostBridgeTest {
    private val gameReminder = mockk<ProductGameReminder>(relaxed = true)
    private val game = ProductId.fromStoredValue("game.dot")

    private fun TestScope.bridge(
        launcher: TrUAPIConfirmationLauncher = mockk(),
    ) = ProductTrUAPIHostBridge(
        hostApiInteractor = mockk(),
        chainHttpClient = OkHttpClient(),
        encryptedPreferences = mockk {
            every { storageIdentifier } returns "bridge-test"
        },
        confirmationLauncher = launcher,
        appLifecycleObserver = mockk(),
        dotNsTldProvider = mockk(),
        pocketCardStore = mockk(),
        context = mockk(),
        appLanguageProvider = object : AppLanguageProvider {
            override val languageTag = flowOf("en-US")
        },
        mediaFactory = mockk<NativeMediaBackendFactory> {
            every { create("game.dot") } returns mockk(relaxed = true)
        },
        permissionRepository = Lazy {
            mockk<ProductPermissionRepository> {
                coEvery { getAllByProduct(game) } returns emptyList()
            }
        },
        permissionChanges = PermissionAuthorizationChanges(),
        productGameReminder = gameReminder,
        scope = CoroutineScope(StandardTestDispatcher(testScheduler)),
    )

    // The callers launch attach into scopes with no handler, so a refusal from the core has to come
    // back as the Result the signature promises rather than as a crash.
    @Test
    fun `a core that refuses to open the execution fails the attach instead of throwing`() = runTest {
        val runtime = mockk<io.parity.truapi.TrUAPIHostRuntime> {
            every { openProductExecution(any(), any(), any(), any(), any(), any()) } throws
                IllegalStateException("loopback port unavailable")
        }
        val outcome = bridge().attach(
            runtime = runtime,
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
        val runtime = mockk<io.parity.truapi.TrUAPIHostRuntime> {
            every { openProductExecution(any(), any(), any(), any(), any(), any()) } answers {
                callbacks = firstArg()
                throw IllegalStateException("captured callbacks without opening a native execution")
            }
        }
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
            for (decision in PermissionDecision.entries) {
                val launcher = mockk<TrUAPIConfirmationLauncher>()
                val prompted = CompletableDeferred<TrUAPIConfirmation>()
                val answer = CompletableDeferred<PermissionDecision>()
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
                answer.complete(decision)
                val expected: Any = if (permissionCallback) decision else decision != PermissionDecision.DENY
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
        for (decision in PermissionDecision.entries) {
            val prompts = mutableListOf<TrUAPIConfirmation>()
            val launcher = mockk<TrUAPIConfirmationLauncher>()
            coEvery { launcher.awaitDecision(capture(prompts)) } returns decision
            val callbacks = callbacks(launcher)

            assertEquals(
                decision,
                callbacks.confirmPermission(review),
            )
            val prompt = prompts.single() as TrUAPIConfirmation.AccountAccess
            assertEquals("game.dot", prompt.requesterProductId)
            assertEquals("target.dot", prompt.targetProductId)
        }
    }

    private suspend fun TestScope.attachCapturingBridge(card: ExpandedCardFace?): HostBridge {
        var opened: HostBridge? = null
        val runtime = mockk<io.parity.truapi.TrUAPIHostRuntime> {
            every { openProductExecution(any(), any(), any(), any(), any(), any()) } answers {
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
