package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import android.content.Context
import uniffi.truapi.AccountAccessReview
import uniffi.truapi.PermissionDecision
import uniffi.truapi.ProfileDisclosureReview
import uniffi.truapi.UserConfirmationReview
import dagger.Lazy
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.PermissionAuthorizationChanges
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.ProductPermissionRepository
import io.paritytech.polkadotapp.feature_settings_api.domain.language.AppLanguageProvider
import kotlinx.coroutines.flow.flowOf
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
import io.paritytech.polkadotapp.test_shared.whenever
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
import org.mockito.Mockito.mock
import org.mockito.Mockito.verify
import org.mockito.Mockito.verifyNoInteractions
import org.mockito.stubbing.Answer
import uniffi.truapi.ExpandedCardFaceOutcome
import uniffi.truapi.HostRejection
import uniffi.truapi.ProductExecutionKind

class ProductTrUAPIHostBridgeTest {
    // The core refuses the open: an unavailable loopback port, or an execution config it rejects.
    private val refusingCore = Answer<Any> { throw IllegalStateException("loopback port unavailable") }

    private val gameReminder = mock(ProductGameReminder::class.java)
    private val game = ProductId.fromStoredValue("game.dot")

    private fun TestScope.bridge(
        launcher: TrUAPIConfirmationLauncher = mock(TrUAPIConfirmationLauncher::class.java),
    ) = ProductTrUAPIHostBridge(
        hostApiInteractor = mock(HostApiInteractor::class.java),
        chainHttpClient = OkHttpClient(),
        encryptedPreferences = mock(EncryptedPreferences::class.java),
        confirmationLauncher = launcher,
        appLifecycleObserver = mock(AppLifecycleObserver::class.java),
        dotNsTldProvider = mock(DotNsTldProvider::class.java),
        pocketCardStore = mock(PocketCardStore::class.java),
        context = mock(Context::class.java),
        appLanguageProvider = object : AppLanguageProvider {
            override val languageTag = flowOf("en-US")
        },
        permissionRepository = Lazy { mock(ProductPermissionRepository::class.java) },
        permissionChanges = PermissionAuthorizationChanges(),
        productGameReminder = gameReminder,
        scope = CoroutineScope(StandardTestDispatcher(testScheduler)),
    )

    // The callers launch attach into scopes with no handler, so a refusal from the core has to come
    // back as the Result the signature promises rather than as a crash.
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
    fun `unsupported profile permission throws through product callbacks while actions fail closed`() = runTest {
        val launcher = mock(TrUAPIConfirmationLauncher::class.java)
        val callbacks = callbacks(launcher)
        val review = UserConfirmationReview.ProfileDisclosure(ProfileDisclosureReview("game.dot"))

        try {
            callbacks.confirmPermission(review)
            fail("An unavailable prompt must not return a durable denial")
        } catch (_: HostRejection.Rejected) {
            // The core maps this callback error to NotDetermined without persisting a decision.
        }
        assertFalse(callbacks.confirmUserAction(review))
        verifyNoInteractions(launcher)
    }

    @Test
    fun `supported permission preserves explicit approval and denial through product callbacks`() = runTest {
        val review = UserConfirmationReview.AccountAccess(AccountAccessReview("game.dot", "target.dot"))
        for (approved in listOf(true, false)) {
            val launcher = mock(TrUAPIConfirmationLauncher::class.java, Answer { approved })
            val callbacks = callbacks(launcher)

            assertEquals(
                if (approved) PermissionDecision.ALLOW_ALWAYS else PermissionDecision.DENY,
                callbacks.confirmPermission(review),
            )
            verify(launcher).awaitDecision(review.toConfirmation("game.dot"))
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

        verify(gameReminder).schedule(game, 1_000)
        verify(gameReminder).cancel(game)
    }

    @Test
    fun `the game bridge rejects a schedule the reminder fails`() = runTest {
        withScheduleOutcome(Result.failure(IllegalStateException("notifications are not allowed")))
        val gameBridge = bridge().gameBridge(game)

        val outcome = runCatching { gameBridge.scheduleReminder(1_000u) }

        assertEquals("notifications are not allowed", (outcome.exceptionOrNull() as? HostRejection.Rejected)?.reason)
    }

    private suspend fun withScheduleOutcome(outcome: Result<Unit>) {
        whenever(gameReminder.schedule(game, 1_000)).thenReturn(outcome)
    }
}
