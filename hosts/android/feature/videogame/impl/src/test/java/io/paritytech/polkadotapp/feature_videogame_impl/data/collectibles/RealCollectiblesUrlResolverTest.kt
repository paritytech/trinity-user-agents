package io.paritytech.polkadotapp.feature_videogame_impl.data.collectibles

import android.net.Uri
import io.mockk.coEvery
import io.mockk.every
import io.mockk.mockk
import io.mockk.mockkObject
import io.mockk.mockkStatic
import io.mockk.unmockkAll
import io.paritytech.polkadotapp.common.utils.FeatureFlags
import io.paritytech.polkadotapp.common.utils.FeatureOption
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsResolver
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.tools_remoteconfig_api.RemoteConfigService
import kotlinx.coroutines.test.runTest
import org.junit.After
import org.junit.Assert.assertSame
import org.junit.Before
import org.junit.Test

class RealCollectiblesUrlResolverTest {
    private val dotNsResolver: DotNsResolver = mockk {
        coEvery { resolveToLocalUri(any()) } returns Result.failure(IllegalStateException("not registered"))
    }

    private val remoteConfigService: RemoteConfigService = mockk {
        coEvery { getSyncedBoolean("collectibles_enabled") } returns Result.success(true)
        coEvery { getSyncedString("collectibles_fallback_url") } returns Result.failure(IllegalStateException("unset"))
    }

    @Before
    fun setUp() {
        mockkObject(FeatureFlags)
        every { FeatureFlags.isEnabled(FeatureOption.COLLECTIBLES) } returns true
        mockkStatic(Uri::class)
    }

    @After
    fun tearDown() = unmockkAll()

    // The Pocket tab shows the collectibles card only when this resolves, and dotNS names live under each
    // network's own suffix, so a fixed suffix hides the card on every other network.
    @Test
    fun `opens the stash page under the active network's suffix`() = runTest {
        for (network in listOf("paseo", "dot")) {
            val stashPage: Uri = mockk()
            every { Uri.parse("https://stash.$network/") } returns stashPage
            coEvery { dotNsResolver.resolveToLocalUri("stash.$network") } returns Result.success(mockk())

            assertSame(network, stashPage, resolverOn(network).resolveUrl())
        }
    }

    private fun resolverOn(network: String): RealCollectiblesUrlResolver {
        val tldProvider: DotNsTldProvider = mockk {
            coEvery { getTld() } returns Result.success(DotNsTld.parse(network)!!)
        }
        return RealCollectiblesUrlResolver(dotNsResolver, tldProvider, remoteConfigService)
    }
}
