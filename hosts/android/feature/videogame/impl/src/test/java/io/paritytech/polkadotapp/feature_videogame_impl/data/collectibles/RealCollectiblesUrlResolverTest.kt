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
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.tools_remoteconfig_api.RemoteConfigService
import kotlinx.coroutines.test.runTest
import org.junit.After
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Before
import org.junit.Test

class RealCollectiblesUrlResolverTest {
    private val remoteConfigService: RemoteConfigService = mockk {
        coEvery { getSyncedBoolean("collectibles_enabled") } returns Result.success(true)
    }

    @Before
    fun setUp() {
        mockkObject(FeatureFlags)
        every { FeatureFlags.isEnabled(FeatureOption.COLLECTIBLES) } returns true
        mockkStatic(Uri::class)
    }

    @After
    fun tearDown() = unmockkAll()

    // dotNS names live under each network's own suffix, so a fixed suffix opens a page that exists on
    // one network only.
    @Test
    fun `opens the stash product under the active network's suffix`() = runTest {
        for (network in listOf("paseo", "dot")) {
            val stashPage = pageAt("https://stash.$network")

            assertSame(network, stashPage, resolverOn(tldReads(network)).resolveUrl())
        }
    }

    // The Pocket tab asks once, often before the people chain answers, and "no page" would hide the card
    // until the screen is built again.
    @Test
    fun `waits out a failed suffix read instead of hiding the card`() = runTest {
        val stashPage = pageAt("https://stash.paseo")

        assertSame(stashPage, resolverOn(tldReads("paseo", failuresFirst = 1)).resolveUrl())
    }

    // The remote switch is how the card is pulled from every installed app without a release.
    @Test
    fun `no page when the remote switch is off`() = runTest {
        pageAt("https://stash.paseo")
        coEvery { remoteConfigService.getSyncedBoolean("collectibles_enabled") } returns Result.success(false)

        assertNull(resolverOn(tldReads("paseo")).resolveUrl())
    }

    private fun pageAt(url: String): Uri = mockk<Uri>().also { every { Uri.parse(url) } returns it }

    private fun tldReads(network: String, failuresFirst: Int = 0): DotNsTldProvider = mockk {
        val failures = List(failuresFirst) { Result.failure<DotNsTld>(IllegalStateException("chain unreachable")) }
        coEvery { getTld() } returnsMany failures + Result.success(DotNsTld.parse(network)!!)
    }

    private fun resolverOn(tldProvider: DotNsTldProvider) = RealCollectiblesUrlResolver(tldProvider, remoteConfigService)
}
