package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import android.content.Context
import dagger.Lazy
import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.parity.truapi.HostBridge
import io.parity.truapi.ProductExecutionKind
import io.parity.truapi.TrUAPIHostRuntime
import io.paritytech.polkadotapp.common.data.storage.preferences.encrypted.EncryptedPreferences
import io.paritytech.polkadotapp.common.presentation.AppLifecycleObserver
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.HostApiInteractor
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.navigation.NavigationPolicy
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.PermissionAuthorizationChanges
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.ProductPermissionRepository
import io.paritytech.polkadotapp.feature_products_impl.domain.pocket.PocketCardStore
import io.paritytech.polkadotapp.feature_settings_api.domain.language.AppLanguageProvider
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.async
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runTest
import okhttp3.OkHttpClient
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test
import org.mockito.Mockito.mock
import org.mockito.stubbing.Answer
import uniffi.truapi.AccountAccessReview
import uniffi.truapi.HostRejection
import uniffi.truapi.PermissionDecision
import uniffi.truapi.ProfileDisclosureReview
import uniffi.truapi.UserConfirmationReview

class ProductTrUAPIHostBridgeTest {
    private val refusingCore = Answer<Any> { throw IllegalStateException("loopback port unavailable") }

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
        mediaFactory = mockk {
            every { create(any()) } returns mockk(relaxed = true)
        },
        productGameReminder = mockk(relaxed = true),
        scope = CoroutineScope(StandardTestDispatcher(testScheduler)),
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
                val expected: Any =
                    if (permissionCallback) decision else decision != PermissionDecision.DENY
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
            coEvery { launcher.awaitDecision(capture(prompts)) } returns
                if (approved) PermissionDecision.ALLOW_ALWAYS else PermissionDecision.DENY
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
}
