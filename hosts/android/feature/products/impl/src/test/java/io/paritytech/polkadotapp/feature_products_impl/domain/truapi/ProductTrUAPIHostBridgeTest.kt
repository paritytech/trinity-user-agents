package io.paritytech.polkadotapp.feature_products_impl.domain.truapi

import io.parity.truapi.TrUAPIHostRuntime
import io.paritytech.polkadotapp.common.presentation.AppLifecycleObserver
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.HostApiInteractor
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.navigation.NavigationPolicy
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runTest
import okhttp3.OkHttpClient
import org.junit.Assert.assertTrue
import org.junit.Test
import org.mockito.Mockito.mock
import org.mockito.stubbing.Answer
import uniffi.truapi.ProductExecutionKind

class ProductTrUAPIHostBridgeTest {
    // The core refuses the open: an unavailable loopback port, or an execution config it rejects.
    private val refusingCore = Answer<Any> { throw IllegalStateException("loopback port unavailable") }

    private fun TestScope.bridge() = ProductTrUAPIHostBridge(
        hostApiInteractor = mock(HostApiInteractor::class.java),
        chainHttpClient = OkHttpClient(),
        secretStorage = io.mockk.mockk(),
        notifications = io.mockk.mockk(),
        localSessionSource = io.mockk.mockk { io.mockk.coEvery { resolve() } returns Result.success(TrUAPILocalSession("test", null)) },
        confirmationLauncher = mock(TrUAPIConfirmationLauncher::class.java),
        appLifecycleObserver = mock(AppLifecycleObserver::class.java),
        dotNsTldProvider = mock(DotNsTldProvider::class.java),
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
            onReadyToInject = {},
        )

        assertTrue(outcome.isFailure)
    }
}
