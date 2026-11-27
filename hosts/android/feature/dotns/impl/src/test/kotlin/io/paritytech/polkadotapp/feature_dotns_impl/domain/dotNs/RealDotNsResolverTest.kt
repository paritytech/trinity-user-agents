package io.paritytech.polkadotapp.feature_dotns_impl.domain.dotNs

import io.paritytech.polkadotapp.common.utils.network.NetworkRetryPolicy
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsLoadProgress
import io.paritytech.polkadotapp.test_shared.testDispatchers
import io.paritytech.polkadotapp.test_shared.whenever
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertTrue
import org.junit.Test
import org.mockito.Mockito.mock
import org.mockito.Mockito.times
import org.mockito.Mockito.verify

class RealDotNsResolverTest {
    private val chainReader: DotNsChainReader = mock()
    private val contentDownloader: DotNsContentDownloader = mock()
    private val builds: DotNsBuilds = mock()

    private val domain = "peopl.testnet"

    @Test
    fun `a domain whose content hash cannot be read reports failure rather than staying silent`() = runTest {
        withContentHashReadFailing()
        val resolver = resolver()

        val result = resolver.resolveToLocalUri(domain)

        assertTrue(result.isFailure)
        assertTrue(resolver.getProgressByDomain(domain).first() is DotNsLoadProgress.Failed)
    }

    @Test
    fun `a lookup that failed is read from the chain again rather than remembered`() = runTest {
        withContentHashReadFailing()
        val resolver = resolver()

        resolver.resolveToLocalUri(domain)
        resolver.resolveToLocalUri(domain)

        verify(chainReader, times(2)).readContentHash(domain, NetworkRetryPolicy.DEFAULT)
    }

    private suspend fun withContentHashReadFailing() {
        whenever(chainReader.readContentHash(domain, NetworkRetryPolicy.DEFAULT))
            .thenReturn(Result.failure(IllegalStateException("Domain $domain is not registered or has no content")))
    }

    private fun TestScope.resolver() = RealDotNsResolver(
        chainReader = chainReader,
        contentDownloader = contentDownloader,
        builds = builds,
        progressRegistry = DotNsLoadProgressRegistry(),
        dispatchers = testDispatchers(),
    )
}
