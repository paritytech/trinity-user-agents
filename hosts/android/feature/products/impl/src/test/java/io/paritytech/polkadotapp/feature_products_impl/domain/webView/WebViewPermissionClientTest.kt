package io.paritytech.polkadotapp.feature_products_impl.domain.webView

import android.net.Uri
import android.webkit.WebResourceRequest
import android.webkit.WebView
import io.mockk.Called
import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.confirmVerified
import io.mockk.every
import io.mockk.mockk
import io.mockk.verify
import io.parity.truapi.TrUAPIProductExecution
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.CallingProductIdProvider
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.ProductPermissionGuard
import io.paritytech.polkadotapp.feature_products_impl.domain.permissions.models.ProductPermission
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
import uniffi.truapi.RemotePermission
import uniffi.truapi.RemotePermissionRequest
import java.net.URI

class WebViewPermissionClientTest {
    private val productId = ProductId.fromStoredValue("product.paseo")
    private var currentProduct = productId
    private val guard = mockk<ProductPermissionGuard>()
    private val execution = mockk<TrUAPIProductExecution>()
    private val tldProvider = mockk<DotNsTldProvider> { every { currentTldOrNull() } returns null }
    private val view = mockk<WebView>()
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
        coEvery { execution.authorizeRemotePermission(permission) } throws IllegalStateException("execution closed")

        assertNotNull(client.shouldInterceptRequest(view, resource("https://api.example.com/data")))

        verifyRustAuthorization()
    }

    @Test
    fun `a different current product cannot borrow the execution authorization`() = runBlocking<Unit> {
        client.useTrUAPIPermissions(productId, execution, scope)
        currentProduct = ProductId.fromStoredValue("other.paseo")

        assertNotNull(client.shouldInterceptRequest(view, resource("https://api.example.com/data")))

        verify { listOf(execution, guard) wasNot Called }
    }

    @Test
    fun `closing the product blocks subsequent HTTP without prompting`() {
        client.useTrUAPIPermissions(productId, execution, scope)
        scope.cancel()

        assertNotNull(client.shouldInterceptRequest(view, resource("https://api.example.com/data")))

        verify { listOf(execution, guard) wasNot Called }
    }

    @Test
    fun `closing the product rejects a late HTTP approval`() = runBlocking<Unit> {
        client.useTrUAPIPermissions(productId, execution, scope)
        val entered = CompletableDeferred<Unit>()
        val decision = CompletableDeferred<Boolean>()
        coEvery { execution.authorizeRemotePermission(permission) } answers {
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

        verify { listOf(execution, guard) wasNot Called }
    }

    @Test
    fun `legacy runtime still consumes its own network permission`() = runBlocking<Unit> {
        val nativePermission = ProductPermission.RemotePermission.NetworkAccess("api.example.com")
        coEvery { guard.consumePermission(productId, nativePermission) } returns true

        assertNull(client.shouldInterceptRequest(view, resource("https://api.example.com/data")))

        coVerify(exactly = 1) { guard.consumePermission(productId, nativePermission) }
        confirmVerified(guard)
        verify { execution wasNot Called }
    }

    private suspend fun withRustDecision(granted: Boolean) {
        client.useTrUAPIPermissions(productId, execution, scope)
        coEvery { execution.authorizeRemotePermission(permission) } returns granted
    }

    private suspend fun verifyRustAuthorization() {
        coVerify(exactly = 1) { execution.authorizeRemotePermission(permission) }
        confirmVerified(execution)
        verify { guard wasNot Called }
    }

    private fun resource(url: String): WebResourceRequest {
        val parsed = URI(url)
        val uri = mockk<Uri>()
        every { uri.scheme } returns parsed.scheme
        every { uri.host } returns parsed.host
        every { uri.port } returns parsed.port
        every { uri.toString() } returns url
        return mockk<WebResourceRequest>().also { request ->
            every { request.url } returns uri
            every { request.isForMainFrame } returns false
        }
    }
}
