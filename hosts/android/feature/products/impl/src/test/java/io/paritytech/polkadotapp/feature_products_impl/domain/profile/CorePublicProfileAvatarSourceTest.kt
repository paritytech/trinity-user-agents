package io.paritytech.polkadotapp.feature_products_impl.domain.profile

import coil.request.ImageRequest
import io.mockk.coEvery
import io.mockk.mockk
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIHostRuntimeProvider
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test

class CorePublicProfileAvatarSourceTest {
    @Test
    fun `runtime failure completes without replacing the letter tile`() = runTest {
        val runtimeProvider: TrUAPIHostRuntimeProvider = mockk {
            coEvery { runtime() } returns Result.failure(IllegalStateException("Runtime unavailable"))
        }
        val source = CorePublicProfileAvatarSource(mockk(), runtimeProvider)

        assertEquals(emptyList<ImageRequest?>(), source.observe("alice.paseo", forceFresh = false).toList())
    }
}
