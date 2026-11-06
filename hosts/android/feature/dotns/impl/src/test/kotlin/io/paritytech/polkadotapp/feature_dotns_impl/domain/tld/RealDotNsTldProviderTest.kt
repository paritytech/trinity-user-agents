package io.paritytech.polkadotapp.feature_dotns_impl.domain.tld

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.mockk
import io.paritytech.polkadotapp.common.utils.RealCoroutineDispatchers
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import io.paritytech.polkadotapp.feature_dotns_impl.data.repository.NetworkSuffixRepository
import io.paritytech.polkadotapp.feature_dotns_impl.data.storage.DotNsTldStorage
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

private val PASEO = DotNsTld.parse("paseo")!!

class RealDotNsTldProviderTest {
    private val networkSuffixRepository: NetworkSuffixRepository = mockk()
    private val storage = FakeDotNsTldStorage()

    private val provider by lazy {
        RealDotNsTldProvider(networkSuffixRepository, storage, RealCoroutineDispatchers())
    }

    @Test
    fun `settles and persists the suffix the chain reports`() = runBlocking<Unit> {
        withReportedSuffix(PASEO)

        val result = provider.getTld()

        assertEquals(PASEO, result.getOrThrow())
        assertEquals(PASEO, storage.stored)
    }

    @Test
    fun `fails without settling when the chain reports no usable suffix`() = runBlocking<Unit> {
        withReportedSuffix(null)

        assertTrue(provider.getTld().isFailure)
        assertNull(storage.stored)
    }

    @Test
    fun `fails without settling when the read fails - then retries on the next call`() = runBlocking<Unit> {
        withFailingRead()

        assertTrue(provider.getTld().isFailure)
        assertNull(storage.stored)

        withReportedSuffix(PASEO)

        assertEquals(PASEO, provider.getTld().getOrThrow())
    }

    @Test
    fun `serves the settled value without a second read`() = runBlocking<Unit> {
        withReportedSuffix(PASEO)

        provider.getTld()
        provider.getTld()

        coVerify(exactly = 1) { networkSuffixRepository.networkSuffix() }
        assertEquals(PASEO, provider.currentTldOrNull())
    }

    @Test
    fun `currentTldOrNull serves the persisted value before the first read settles`() = runBlocking<Unit> {
        withFailingRead()
        storage.putTld(PASEO)

        assertEquals(PASEO, provider.currentTldOrNull())
    }

    @Test
    fun `currentTldOrNull is null without a settled or persisted value`() = runBlocking<Unit> {
        withFailingRead()

        assertNull(provider.currentTldOrNull())
    }

    private suspend fun withReportedSuffix(tld: DotNsTld?) {
        coEvery { networkSuffixRepository.networkSuffix() } returns Result.success(tld)
    }

    private suspend fun withFailingRead() {
        coEvery { networkSuffixRepository.networkSuffix() } returns Result.failure(IllegalStateException("rpc down"))
    }

    private class FakeDotNsTldStorage : DotNsTldStorage {
        var stored: DotNsTld? = null

        override fun getTld(): DotNsTld? = stored

        override fun putTld(tld: DotNsTld) {
            stored = tld
        }
    }
}
