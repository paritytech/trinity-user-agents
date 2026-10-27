package io.paritytech.polkadotapp.feature_products_impl.domain.product

import io.mockk.coVerify
import io.mockk.mockk
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsResolver
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.truapi.ProductId
import uniffi.truapi.ProductIdException

class ProductWorkerArchiveTest {
    @Test
    fun `invalid executable identity returns a failure without resolving content`() = runTest {
        val resolver = mockk<DotNsResolver>()
        val archive = ProductWorkerArchive(resolver)

        val result = archive.file(ProductId.parse("localhost"), "worker.js")

        assertTrue(result.exceptionOrNull() is ProductIdException)
        coVerify(exactly = 0) { resolver.resolveToLocalUri(any()) }
    }
}
