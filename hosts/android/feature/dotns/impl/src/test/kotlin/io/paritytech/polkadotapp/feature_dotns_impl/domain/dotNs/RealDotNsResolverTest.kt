package io.paritytech.polkadotapp.feature_dotns_impl.domain.dotNs

import io.paritytech.polkadotapp.common.data.telemetry.HostEvent
import io.paritytech.polkadotapp.common.data.telemetry.HostEventOrigin
import io.paritytech.polkadotapp.common.data.telemetry.HostLoadFailureTelemetry
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
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.mockito.Mockito.mock
import org.mockito.Mockito.times
import org.mockito.Mockito.verify
import java.io.File
import java.io.IOException

class RealDotNsResolverTest {
    private val contractApi: DotNsContractApi = mock()
    private val carFetcher: CarFetcher = mock()
    private val contentStorage: DotNsContentStorage = mock()
    private val contentHashOverrides: ContentHashOverrides = mock()

    private val events = mutableListOf<HostEvent>()

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

    // A failed lookup is usually a read that did not land, such as a device briefly offline.
    // Remembered, it would keep the product unreachable until the app restarts, network or not.
    @Test
    fun `a lookup that failed is read from the chain again rather than remembered`() = runTest {
        whenever(contentHashOverrides.getContentHashOverride(domain)).thenReturn(null)
        whenever(contractApi.resolveContentHash(domain)).thenReturn(Result.failure(IllegalStateException("no node")))
        val resolver = resolver()

        resolver.resolveToLocalUri(domain)
        resolver.resolveToLocalUri(domain)

        verify(contractApi, times(2)).resolveContentHash(domain)
    }

    // The chain read is the resolve stage; a failed lookup is retried on every request, and one
    // failing domain is one event.
    @Test
    fun `a failed chain read is reported once under the resolve stage however often it is retried`() = runTest {
        whenever(contentHashOverrides.getContentHashOverride(domain)).thenReturn(null)
        whenever(contractApi.resolveContentHash(domain)).thenReturn(Result.failure(IllegalStateException("no node")))
        val resolver = resolver()

        repeat(3) { resolver.resolveToLocalUri(domain) }

        assertEquals("resolve", events.single().tags["load.stage"])
        assertEquals(domain, events.single().tags["dotns.domain"])
    }

    // The download is a different outage from the chain read, and the stage tag is what tells them apart.
    @Test
    fun `a failed archive download is reported under the bulletin stage, not as a failed chain read`() = runTest {
        whenever(contentHashOverrides.getContentHashOverride(domain)).thenReturn("0x1220abcd")
        val gatewayDown = object : CarFetcher {
            override suspend fun fetchCar(contentHash: ByteArray) = Result.failure<ByteArray>(IOException("gateway down"))

            override suspend fun fetchCarToFile(contentHash: ByteArray, onProgress: (Long, Long?) -> Unit) =
                Result.failure<File>(IOException("gateway down"))
        }
        val resolver = resolver(gatewayDown)

        val result = resolver.resolveToLocalUri(domain)

        assertTrue(result.isFailure)
        assertEquals("bulletin", events.single().tags["load.stage"])
        assertEquals(domain, events.single().tags["dotns.domain"])
    }

    private fun TestScope.resolver(carFetcher: CarFetcher = this@RealDotNsResolverTest.carFetcher) = RealDotNsResolver(
        contractApi = contractApi,
        carFetcher = carFetcher,
        contentStorage = contentStorage,
        contentHashOverrides = contentHashOverrides,
        dispatchers = testDispatchers(),
        loadFailures = HostLoadFailureTelemetry(
            originProvider = { HostEventOrigin("debug", "gp", "1.2.3", "android", "15", 123L) },
            capture = { events += it() },
        ),
    )
}
