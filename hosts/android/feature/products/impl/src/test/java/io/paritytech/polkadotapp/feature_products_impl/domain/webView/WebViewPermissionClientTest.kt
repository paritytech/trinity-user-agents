package io.paritytech.polkadotapp.feature_products_impl.domain.webView

import android.net.Uri
import android.webkit.WebResourceRequest
import android.webkit.WebView
import io.parity.truapi.TrUAPIProductExecution
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.CallingProductIdProvider
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.ProductPermissionGuard
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.ProductPermission
import io.paritytech.polkadotapp.test_shared.whenever
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.async
import kotlinx.coroutines.cancel
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.After
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Test
import org.mockito.Mockito.mock
import org.mockito.Mockito.verify
import org.mockito.Mockito.verifyNoInteractions
import org.mockito.Mockito.verifyNoMoreInteractions
import uniffi.truapi.RemotePermission
import uniffi.truapi.RemotePermissionRequest
import java.net.URI

class WebViewPermissionClientTest {
    private val productId = ProductId.fromStoredValue("product.paseo")
    private var currentProduct = productId
    private val guard = mock(ProductPermissionGuard::class.java)
    private val execution = mock(TrUAPIProductExecution::class.java)
    private val tldProvider = mock(DotNsTldProvider::class.java)
    private val view = mock(WebView::class.java)
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Unconfined)
    private val client = WebViewPermissionClient(
        CallingProductIdProvider { Result.success(currentProduct) },
        "http://127.0.0.1:4000",
        guard,
        tldProvider,
    )
    private val permission = RemotePermissionRequest(RemotePermission.Remote(listOf("api.example.com")))

    @After
    fun tearDown() {
        scope.cancel()
    }

    @Test
    fun `external resources use Rust once without consuming native grants`() = runBlocking<Unit> {
        withRustDecision(true)

        assertNull(client.shouldInterceptRequest(view, resource("https://api.example.com/script.js")))

        verifyRustAuthorization()
    }

    @Test
    fun `Rust denial blocks HTTP without consulting native grants`() = runBlocking<Unit> {
        withRustDecision(false)

        assertNotNull(client.shouldInterceptRequest(view, resource("https://api.example.com/image.png")))

        verifyRustAuthorization()
    }

    @Test
    fun `authorization errors block HTTP without falling back to native grants`() = runBlocking<Unit> {
        client.useTrUAPIPermissions(productId, execution, scope)
        whenever(execution.authorizeRemotePermission(permission)).thenThrow(IllegalStateException("execution closed"))

        assertNotNull(client.shouldInterceptRequest(view, resource("https://api.example.com/data")))

        verifyRustAuthorization()
    }

    @Test
    fun `a different current product cannot borrow the execution authorization`() = runBlocking<Unit> {
        client.useTrUAPIPermissions(productId, execution, scope)
        currentProduct = ProductId.fromStoredValue("other.paseo")

        assertNotNull(client.shouldInterceptRequest(view, resource("https://api.example.com/data")))

        verifyNoInteractions(execution, guard)
    }

    @Test
    fun `closing the product blocks subsequent HTTP without prompting`() {
        client.useTrUAPIPermissions(productId, execution, scope)
        scope.cancel()

        assertNotNull(client.shouldInterceptRequest(view, resource("https://api.example.com/data")))

        verifyNoInteractions(execution, guard)
    }

    @Test
    fun `closing the product rejects a late HTTP approval`() = runBlocking<Unit> {
        client.useTrUAPIPermissions(productId, execution, scope)
        val entered = CompletableDeferred<Unit>()
        val decision = CompletableDeferred<Boolean>()
        whenever(execution.authorizeRemotePermission(permission)).thenAnswer {
            entered.complete(Unit)
            runBlocking { decision.await() }
        }
        val response = async(Dispatchers.Default) {
            client.shouldInterceptRequest(view, resource("https://api.example.com/data"))
        }
        withTimeout(5_000) { entered.await() }

        scope.cancel()
        decision.complete(true)

        assertNotNull(withTimeout(5_000) { response.await() })
        verifyRustAuthorization()
    }

    @Test
    fun `worker executable resources keep their first party exemption`() {
        client.useTrUAPIPermissions(productId, execution, scope)

        assertNull(client.shouldInterceptRequest(view, resource("http://127.0.0.1:4000/worker.js")))

        verifyNoInteractions(execution, guard)
    }

    @Test
    fun `legacy runtime still consumes its own network permission`() = runBlocking<Unit> {
        val nativePermission = ProductPermission.RemotePermission.NetworkAccess("api.example.com")
        whenever(guard.consumePermission(productId, nativePermission)).thenReturn(true)

        assertNull(client.shouldInterceptRequest(view, resource("https://api.example.com/data")))

        verify(guard).consumePermission(productId, nativePermission)
        verifyNoMoreInteractions(guard)
        verifyNoInteractions(execution)
    }

    private suspend fun withRustDecision(granted: Boolean) {
        client.useTrUAPIPermissions(productId, execution, scope)
        whenever(execution.authorizeRemotePermission(permission)).thenReturn(granted)
    }

    private suspend fun verifyRustAuthorization() {
        verify(execution).authorizeRemotePermission(permission)
        verifyNoMoreInteractions(execution)
        verifyNoInteractions(guard)
    }

    private fun resource(url: String): WebResourceRequest {
        val parsed = URI(url)
        val uri = mock(Uri::class.java)
        whenever(uri.scheme).thenReturn(parsed.scheme)
        whenever(uri.host).thenReturn(parsed.host)
        whenever(uri.port).thenReturn(parsed.port)
        whenever(uri.toString()).thenReturn(url)
        return mock(WebResourceRequest::class.java).also { request ->
            whenever(request.url).thenReturn(uri)
            whenever(request.isForMainFrame).thenReturn(false)
        }
    }
}
