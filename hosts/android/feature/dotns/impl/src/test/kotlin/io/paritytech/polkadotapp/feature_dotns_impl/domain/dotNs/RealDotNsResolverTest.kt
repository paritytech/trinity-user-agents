package io.paritytech.polkadotapp.feature_dotns_impl.domain.dotNs

import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsLoadProgress
import io.paritytech.polkadotapp.feature_dotns_impl.data.contract.DotNsContractApi
import io.paritytech.polkadotapp.feature_dotns_impl.data.ipfs.CarFetcher
import io.paritytech.polkadotapp.feature_dotns_impl.data.storage.ContentHashOverrides
import io.paritytech.polkadotapp.feature_dotns_impl.data.storage.DotNsContentStorage
import io.paritytech.polkadotapp.test_shared.testDispatchers
import io.paritytech.polkadotapp.test_shared.whenever
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertTrue
import org.junit.Test
import org.mockito.Mockito.mock

class RealDotNsResolverTest {
    private val contractApi: DotNsContractApi = mock()
    private val carFetcher: CarFetcher = mock()
    private val contentStorage: DotNsContentStorage = mock()
    private val contentHashOverrides: ContentHashOverrides = mock()

    private val domain = "peopl.testnet"

    // A host screen shows a spinner for as long as progress says the content is on its way. A domain
    // with nothing registered behind it never downloads and never unpacks, so unless the failure is
    // reported the spinner is the last thing the user ever sees.
    @Test
    fun `a domain with no content registered reports failure rather than staying silent`() = runTest {
        whenever(contentHashOverrides.getContentHashOverride(domain)).thenReturn(null)
        whenever(contractApi.resolveContentHash(domain)).thenReturn(Result.success(null))
        val resolver = resolver()

        val result = resolver.resolveToLocalUri(domain)

        assertTrue(result.isFailure)
        assertTrue(resolver.getProgressByDomain(domain).first() is DotNsLoadProgress.Failed)
    }

    @Test
    fun `a chain that cannot be reached reports failure too`() = runTest {
        whenever(contentHashOverrides.getContentHashOverride(domain)).thenReturn(null)
        whenever(contractApi.resolveContentHash(domain)).thenReturn(Result.failure(IllegalStateException("no node")))
        val resolver = resolver()

        val result = resolver.resolveToLocalUri(domain)

        assertTrue(result.isFailure)
        assertTrue(resolver.getProgressByDomain(domain).first() is DotNsLoadProgress.Failed)
    }

    private fun TestScope.resolver() = RealDotNsResolver(
        contractApi = contractApi,
        carFetcher = carFetcher,
        contentStorage = contentStorage,
        contentHashOverrides = contentHashOverrides,
        dispatchers = testDispatchers(),
    )
}
