package io.paritytech.polkadotapp.feature_products_impl.domain.webView

import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import io.paritytech.polkadotapp.feature_dotns_api.presentation.ArchiveContent
import io.paritytech.polkadotapp.feature_products_impl.domain.bundled.BundledProducts
import io.paritytech.polkadotapp.feature_products_impl.domain.bundled.appHost
import io.paritytech.polkadotapp.feature_products_impl.domain.bundled.appRoot
import io.paritytech.polkadotapp.feature_products_impl.domain.bundled.workerHost
import io.paritytech.polkadotapp.feature_products_impl.domain.bundled.workerRoot
import uniffi.truapi.BundleEntry

/**
 * Serves one bundled product's files to a WebView under [host], by the same rules as a published archive, so its
 * relative imports and client-side routes resolve as they would once published. Other hosts are left to the
 * WebView's other clients.
 */
class BundledProductContent private constructor(
    val host: String,
    private val root: String,
    private val content: ArchiveContent,
) {
    companion object {
        fun worker(entry: BundleEntry, products: BundledProducts) =
            BundledProductContent(entry.workerHost, entry.workerRoot, ArchiveContent(products.files, emptyMap()))

        fun app(entry: BundleEntry, products: BundledProducts, mainDocumentResponseHeaders: Map<String, String>) =
            BundledProductContent(entry.appHost, entry.appRoot, ArchiveContent(products.files, mainDocumentResponseHeaders))
    }

    fun serves(host: String?): Boolean = host.equals(this.host, ignoreCase = true)

    fun intercept(request: WebResourceRequest): WebResourceResponse? =
        if (serves(request.url.host)) content.respond(root, request) else null
}
