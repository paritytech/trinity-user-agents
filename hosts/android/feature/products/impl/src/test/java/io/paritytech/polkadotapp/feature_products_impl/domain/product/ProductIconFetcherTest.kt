package io.paritytech.polkadotapp.feature_products_impl.domain.product

import android.net.Uri
import coil.ImageLoader
import coil.fetch.FetchResult
import coil.fetch.Fetcher
import coil.request.Options
import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_products_api.model.Executables
import io.paritytech.polkadotapp.feature_products_api.model.Product
import io.paritytech.polkadotapp.feature_products_api.model.ProductIcon
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_api.model.ResolvedProduct
import io.paritytech.polkadotapp.feature_products_impl.domain.usecase.ResolveProductUseCase
import io.paritytech.polkadotapp.tools_ipfs_api.Cid
import io.paritytech.polkadotapp.tools_ipfs_api.IpfsContentLookup
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test

private const val CID = "QmXoypizjW3WknFiJnKLwHCnL72vedxjQkDDP1mXWo6uco"
private const val GATEWAY_URL = "https://gateway.example/ipfs/$CID"

class ProductIconFetcherTest {
    private val productId = ProductId.fromStoredValue("dim2.paseo")
    private val resolveProduct: ResolveProductUseCase = mockk()
    private val ipfsContentLookup: IpfsContentLookup = mockk {
        coEvery { getIpfsLinkFor(CID) } returns Result.success(GATEWAY_URL)
    }
    private val options: Options = mockk()
    private val imageLoader: ImageLoader = mockk()
    private val dotNsTldProvider: DotNsTldProvider = mockk {
        coEvery { getTld() } returns Result.success(requireNotNull(DotNsTld.parse("paseo")))
    }
    private val factory = ProductIconFetcher.Factory(dotNsTldProvider, resolveProduct, ipfsContentLookup)

    // Jollity is placed by the host before anything resolves it, so its chat names only the product
    // and the icon is found when the row is drawn.
    @Test
    fun `a product icon uri loads the icon that product's manifest declares`() = runTest {
        withManifestIcon(ProductIcon(Cid.decode(CID), ProductIcon.Format.PNG))
        val gatewayResult = withGatewayImage()

        val fetcher = factory.create(uriOf(productId.productIconUri()), options, imageLoader)

        assertSame(gatewayResult, fetcher?.fetch())
    }

    @Test(expected = IllegalStateException::class)
    fun `a product without a manifest icon fails, leaving its letter avatar in place`() = runTest {
        withManifestIcon(icon = null)

        factory.create(uriOf(productId.productIconUri()), options, imageLoader)?.fetch()
    }

    // Anything can build an image model, so a name that is not a product of this network is never resolved.
    @Test
    fun `a name outside this network's products is not resolved`() = runTest {
        val loaded = runCatching { factory.create(uriOf("product-icon:evil.dot"), options, imageLoader)?.fetch() }

        assertTrue(loaded.exceptionOrNull() is IllegalArgumentException)
        coVerify(exactly = 0) { resolveProduct.resolve(any()) }
    }

    @Test
    fun `other images are left to the fetchers that serve them`() {
        assertNull(factory.create(uriOf("https://example.com/icon.png"), options, imageLoader))
    }

    private fun withManifestIcon(icon: ProductIcon?) {
        val product = Product(productId, "Jollity", icon)
        val executables = Executables(app = null, widget = null, worker = null)
        coEvery { resolveProduct.resolve(productId) } returns Result.success(ResolvedProduct(product, null, executables))
    }

    private fun withGatewayImage(): FetchResult {
        val result = mockk<FetchResult>()
        val gatewayUri = mockk<Uri>()
        val gatewayFetcher = mockk<Fetcher> { coEvery { fetch() } returns result }
        every { imageLoader.components } returns mockk {
            every { map(GATEWAY_URL, options) } returns gatewayUri
            every { newFetcher(gatewayUri, options, imageLoader) } returns (gatewayFetcher to 0)
        }
        return result
    }

    // android.net.Uri has no JVM implementation in unit tests; this splits an opaque URI the way it does.
    private fun uriOf(value: String): Uri = mockk {
        every { scheme } returns value.substringBefore(':')
        every { schemeSpecificPart } returns value.substringAfter(':')
    }
}
