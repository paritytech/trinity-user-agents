package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import android.content.Context
import io.parity.truapi.HostBridge
import uniffi.truapi.AccountAccessReview
import uniffi.truapi.HostRejection
import uniffi.truapi.PermissionDecision
import uniffi.truapi.ProfileDisclosureReview
import uniffi.truapi.UserConfirmationReview
import io.paritytech.polkadotapp.feature_settings_api.domain.language.AppLanguageProvider
import kotlinx.coroutines.flow.flowOf
import uniffi.truapi.ProductExecutionKind
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
import uniffi.truapi.WsBridgeEndpoint

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
        mediaFactory = mock(io.paritytech.polkadotapp.tools_media_connection_impl.nativeMedia.NativeMediaBackendFactory::class.java).also {
            org.mockito.Mockito.`when`(it.create("game.dot"))
                .thenReturn(mock(io.paritytech.polkadotapp.tools_media_connection_impl.nativeMedia.NativeMediaBackend::class.java))
        },
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
}
