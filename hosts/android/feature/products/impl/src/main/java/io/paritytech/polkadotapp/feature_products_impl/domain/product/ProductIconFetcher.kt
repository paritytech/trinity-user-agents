package io.paritytech.polkadotapp.feature_products_impl.domain.product

import android.net.Uri
import coil.ComponentRegistry
import coil.ImageLoader
import coil.fetch.FetchResult
import coil.fetch.Fetcher
import coil.request.Options
import io.paritytech.polkadotapp.common.data.image.ImageLoaderComponentRegistrar
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_products_api.model.productIdFromHost
import io.paritytech.polkadotapp.feature_products_impl.domain.usecase.ResolveProductUseCase
import io.paritytech.polkadotapp.tools_ipfs_api.IpfsContentLookup
import uniffi.truapi.ProductId
import javax.inject.Inject

private const val PRODUCT_ICON_SCHEME = "product-icon"

/** An image model for the icon [this] product's manifest declares, resolved only once it is drawn. */
fun ProductId.productIconUri(): String = "$PRODUCT_ICON_SCHEME:$value"

/**
 * Loads a [productIconUri] from the IPFS gateway, after resolving the manifest that names the icon. The
 * product is read from the image model, which anything can build, so it is validated against the TLD.
 */
internal class ProductIconFetcher(
    private val productName: String,
    private val dotNsTldProvider: DotNsTldProvider,
    private val resolveProduct: ResolveProductUseCase,
    private val ipfsContentLookup: IpfsContentLookup,
    private val options: Options,
    private val imageLoader: ImageLoader,
) : Fetcher {
    override suspend fun fetch(): FetchResult? {
        val tld = dotNsTldProvider.getTld().getOrThrow()
        val productId = productIdFromHost(productName, tld).getOrThrow()
        val icon = resolveProduct.resolve(productId).getOrNull()?.product?.icon
            ?: throw IllegalStateException("${productId.value} has no manifest icon")
        val gatewayUrl = ipfsContentLookup.gatewayUrlOf(icon)
            ?: throw IllegalStateException("No IPFS gateway to load the icon of ${productId.value}")
        val components = imageLoader.components

        return components.newFetcher(components.map(gatewayUrl, options), options, imageLoader)?.first?.fetch()
    }

    class Factory(
        private val dotNsTldProvider: DotNsTldProvider,
        private val resolveProduct: ResolveProductUseCase,
        private val ipfsContentLookup: IpfsContentLookup,
    ) : Fetcher.Factory<Uri> {
        override fun create(data: Uri, options: Options, imageLoader: ImageLoader): Fetcher? {
            if (data.scheme != PRODUCT_ICON_SCHEME) return null

            return ProductIconFetcher(data.schemeSpecificPart, dotNsTldProvider, resolveProduct, ipfsContentLookup, options, imageLoader)
        }
    }
}

internal class ProductIconImageLoaderRegistrar @Inject constructor(
    private val dotNsTldProvider: DotNsTldProvider,
    private val resolveProduct: ResolveProductUseCase,
    private val ipfsContentLookup: IpfsContentLookup,
) : ImageLoaderComponentRegistrar {
    override fun register(builder: ComponentRegistry.Builder) {
        builder.add(ProductIconFetcher.Factory(dotNsTldProvider, resolveProduct, ipfsContentLookup))
    }
}
