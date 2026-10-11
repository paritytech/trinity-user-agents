package io.paritytech.polkadotapp.feature_products_impl.domain.product

import android.net.Uri
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsResolver
import io.paritytech.polkadotapp.feature_products_api.domain.runtime.ProductRuntimeSettings
import io.paritytech.polkadotapp.feature_products_api.model.ExecutableKind
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.webView.ProductServingHostResolver
import io.paritytech.polkadotapp.test_shared.thenThrowUnsafe
import io.paritytech.polkadotapp.test_shared.whenever
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.mockito.ArgumentMatchers.anyString
import org.mockito.Mockito.mock
import org.mockito.Mockito.never
import org.mockito.Mockito.verify

class RealProductContentWarmUpTest {
    private val dotNsResolver: DotNsResolver = mock()
    private val servingHostResolver: ProductServingHostResolver = mock()
    private var truapiRuntime = true
    private val runtimeSettings = object : ProductRuntimeSettings {
        override fun isTrUAPIRuntimeEnabled() = truapiRuntime

        override fun setTrUAPIRuntimeEnabled(enabled: Boolean) {
            truapiRuntime = enabled
        }
    }
    private val warmUp = RealProductContentWarmUp(dotNsResolver, servingHostResolver, runtimeSettings)

    private val productId = ProductId.fromStoredValue("game.dot")
    private val archive: Uri = mock()

    // The archive is keyed by the host that serves the card, which for a manifest product is its
    // `widget` executable. Warming any other host would fill a different entry and leave the tap
    // paying for the download after all.
    @Test
    fun `warms the archive of the host that actually serves the product`() = runBlocking {
        whenever(servingHostResolver.servingHostFor("game.dot", ExecutableKind.WIDGET)).thenReturn("widget.game.dot")
        whenever(dotNsResolver.resolveToLocalUri("widget.game.dot")).thenReturn(Result.success(archive))

        val result = warmUp.warmUp(productId)

        verify(dotNsResolver).resolveToLocalUri("widget.game.dot")
        assertTrue(result.isSuccess)
    }

    // The native runtime, which release builds run, still serves the app under a card. Warming the
    // widget there would download an archive that card never loads.
    @Test
    fun `on the native runtime it warms the app archive the card loads`() = runBlocking {
        truapiRuntime = false
        whenever(servingHostResolver.servingHostFor("game.dot", ExecutableKind.APP)).thenReturn("app.game.dot")
        whenever(dotNsResolver.resolveToLocalUri("app.game.dot")).thenReturn(Result.success(archive))

        val result = warmUp.warmUp(productId)

        verify(dotNsResolver).resolveToLocalUri("app.game.dot")
        assertTrue(result.isSuccess)
    }

    // Warming runs while the card is still travelling and nothing on screen waits for it, so a
    // failure here must leave the tap to fail, or succeed, on its own terms.
    @Test
    fun `an archive that cannot be fetched is reported rather than thrown`() = runBlocking {
        val failure = IllegalStateException("chain unreachable")
        whenever(servingHostResolver.servingHostFor("game.dot", ExecutableKind.WIDGET)).thenReturn("widget.game.dot")
        whenever(dotNsResolver.resolveToLocalUri("widget.game.dot")).thenReturn(Result.failure(failure))

        val result = warmUp.warmUp(productId)

        assertEquals(failure, result.exceptionOrNull())
    }

    @Test
    fun `a product whose serving host cannot be worked out is not resolved`() = runBlocking {
        whenever(servingHostResolver.servingHostFor("game.dot", ExecutableKind.WIDGET)).thenThrowUnsafe(IllegalStateException("no manifest"))

        val result = warmUp.warmUp(productId)

        verify(dotNsResolver, never()).resolveToLocalUri(anyString())
        assertTrue(result.isFailure)
    }
}
