package io.paritytech.polkadotapp.feature_dotns_api.presentation

import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsLoadProgress
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsResolver
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Test

class DotNsContentLoaderTest {
    private val resolver: DotNsResolver = mockk()

    private val domain = "app.game.dot"

    // Opening a product with a new build downloads it before the page loads. Without its progress the page looks stuck.
    @Test
    fun `an update shows the progress of its download`() = runBlocking<Unit> {
        val downloading = DotNsLoadProgress.Downloading(0.5f)
        every { resolver.getProgressByDomain(domain) } returns flowOf(downloading)
        coEvery { resolver.updateToLatest(domain) } returns Result.success(mockk())
        val loader = DotNsContentLoader(resolver)

        loader.updateToLatest(domain)

        assertEquals(downloading, loader.loadProgress.first())
    }
}
