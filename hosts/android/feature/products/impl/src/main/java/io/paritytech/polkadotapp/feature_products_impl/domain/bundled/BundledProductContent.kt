package io.paritytech.polkadotapp.feature_products_impl.domain.bundled

import android.webkit.MimeTypeMap
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import java.io.ByteArrayInputStream
import java.util.Locale

/**
 * Serves one bundled product's files to a WebView under [host], the way dotNS serves a published archive,
 * so its relative imports and client-side routes resolve as they would once published. Requests for any
 * other host are left to the WebView's other clients.
 */
class BundledProductContent(
    val host: String,
    private val root: String,
    private val products: BundledProducts,
    private val mainDocumentResponseHeaders: Map<String, String>,
) {
    companion object {
        fun worker(product: BundledProduct, products: BundledProducts) =
            BundledProductContent(product.workerHost, product.workerRoot, products, emptyMap())

        fun app(product: BundledProduct, products: BundledProducts, mainDocumentResponseHeaders: Map<String, String>) =
            BundledProductContent(product.appHost, product.appRoot, products, mainDocumentResponseHeaders)

        // The platform table misses some of these on older releases, and a module script or wasm
        // served as octet-stream does not run.
        private val KNOWN_TYPES = mapOf(
            "js" to "text/javascript",
            "mjs" to "text/javascript",
            "css" to "text/css",
            "html" to "text/html",
            "json" to "application/json",
            "wasm" to "application/wasm",
            "svg" to "image/svg+xml",
        )
    }

    fun serves(host: String?): Boolean = host.equals(this.host, ignoreCase = true)

    fun intercept(request: WebResourceRequest): WebResourceResponse? {
        val url = request.url
        if (!serves(url.host)) return null

        val path = products.assetPath(root, url.path.orEmpty(), request.isForMainFrame) ?: return notFound()
        val stream = products.open(path) ?: return notFound()
        val mimeType = mimeType(path)

        return if (request.isForMainFrame && mainDocumentResponseHeaders.isNotEmpty()) {
            WebResourceResponse(mimeType, "UTF-8", 200, "OK", mainDocumentResponseHeaders, stream)
        } else {
            WebResourceResponse(mimeType, "UTF-8", stream)
        }
    }

    private fun mimeType(path: String): String {
        val extension = path.substringAfterLast('.', "").lowercase(Locale.ROOT)
        return KNOWN_TYPES[extension]
            ?: MimeTypeMap.getSingleton().getMimeTypeFromExtension(extension)
            ?: "application/octet-stream"
    }

    private fun notFound() = WebResourceResponse("text/plain", "UTF-8", ByteArrayInputStream("Not Found".toByteArray())).apply {
        setStatusCodeAndReasonPhrase(404, "Not Found")
    }
}
