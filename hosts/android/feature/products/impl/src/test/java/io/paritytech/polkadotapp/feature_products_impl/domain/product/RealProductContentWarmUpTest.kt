package io.paritytech.polkadotapp.feature_products_impl.domain.product

import android.net.Uri
import io.paritytech.polkadotapp.common.data.telemetry.HostLoadContext
import io.paritytech.polkadotapp.common.data.telemetry.HostLoadTrigger
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsLoadProgress
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsResolver
import io.paritytech.polkadotapp.feature_products_api.domain.runtime.ProductRuntimeSettings
import io.paritytech.polkadotapp.feature_products_impl.domain.webView.ProductServingHostResolver
import io.paritytech.polkadotapp.test_shared.thenThrowUnsafe
import io.paritytech.polkadotapp.test_shared.whenever
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.mockito.ArgumentMatchers.anyString
import org.mockito.Mockito.mock
import org.mockito.Mockito.never
import org.mockito.Mockito.verify
import uniffi.truapi.ExecutableKind
import uniffi.truapi.ProductId

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

    private val productId = ProductId.parse("game.dot").getOrThrow()
    private val archive: Uri = mock()

    // The archive is keyed by the host that serves the card, which for a manifest product is its
    // `widget` executable. Warming any other host would fill a different entry and leave the tap
    // paying for the download after all.
    @Test
    fun `warms the archive of the host that actually serves the product`() = runBlocking {
        whenever(servingHostResolver.servingHostFor("game.dot", ExecutableKind.WIDGET)).thenReturn(Result.success("widget.game.dot"))
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
        whenever(servingHostResolver.servingHostFor("game.dot", ExecutableKind.APP)).thenReturn(Result.success("app.game.dot"))
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
        whenever(servingHostResolver.servingHostFor("game.dot", ExecutableKind.WIDGET)).thenReturn(Result.success("widget.game.dot"))
        whenever(dotNsResolver.resolveToLocalUri("widget.game.dot")).thenReturn(Result.failure(failure))

        val result = warmUp.warmUp(productId)

        assertEquals(failure, result.exceptionOrNull())
    }

    // A warm-up runs before the page it is for, so its fetch must not be reported as that page's open.
    @Test
    fun `the archive is fetched as a prewarm`() = runBlocking {
        whenever(servingHostResolver.servingHostFor("game.dot", ExecutableKind.WIDGET)).thenReturn(Result.success("widget.game.dot"))
        val triggers = mutableListOf<HostLoadTrigger?>()
        val recording = object : DotNsResolver {
            override suspend fun resolveToLocalUri(dotNsName: String): Result<Uri> {
                triggers += currentCoroutineContext()[HostLoadContext]?.trigger
                return Result.success(archive)
            }

            override suspend fun getMetadataEntry(dotNsName: String, key: String): Result<String?> = Result.success(null)

            override fun getProgressByDomain(dotNsName: String): Flow<DotNsLoadProgress> = emptyFlow()

            override suspend fun clearCache() = Unit
        }

        RealProductContentWarmUp(recording, servingHostResolver, runtimeSettings).warmUp(productId)

        assertEquals(listOf<HostLoadTrigger?>(HostLoadTrigger.PREWARM), triggers)
    }

    @Test
    fun `a product whose serving host cannot be worked out is not resolved`() = runBlocking {
        whenever(servingHostResolver.servingHostFor("game.dot", ExecutableKind.WIDGET)).thenThrowUnsafe(IllegalStateException("no manifest"))

        val result = warmUp.warmUp(productId)

        verify(dotNsResolver, never()).resolveToLocalUri(anyString())
        assertTrue(result.isFailure)
    }
}
