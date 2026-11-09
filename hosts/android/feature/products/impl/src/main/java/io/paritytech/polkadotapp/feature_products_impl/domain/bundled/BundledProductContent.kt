package io.paritytech.polkadotapp.feature_products_impl.domain.bundled

import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import uniffi.truapi.mimeFor
import java.io.ByteArrayInputStream

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
    }

    fun serves(host: String?): Boolean = host.equals(this.host, ignoreCase = true)

    /** What a request answers with, read off the shipped files; split from [intercept] so it can be checked off-device. */
    sealed interface Answer {
        data class File(val path: String, val mimeType: String, val headers: Map<String, String>) : Answer

        data object NotFound : Answer
    }

    fun intercept(request: WebResourceRequest): WebResourceResponse? =
        when (val answer = answer(request.url.host, request.url.path.orEmpty(), request.isForMainFrame)) {
            null -> null
            Answer.NotFound -> notFound()
            is Answer.File -> products.open(answer.path)?.let { stream ->
                if (answer.headers.isEmpty()) {
                    WebResourceResponse(answer.mimeType, "UTF-8", stream)
                } else {
                    WebResourceResponse(answer.mimeType, "UTF-8", 200, "OK", answer.headers, stream)
                }
            } ?: notFound()
        }

    /** Null for a host this product is not served under, which is left to the WebView's other clients. */
    fun answer(host: String?, requestPath: String, isMainFrame: Boolean): Answer? {
        if (!serves(host)) return null

        val path = products.assetPath(root, requestPath, isMainFrame) ?: return Answer.NotFound
        val headers = if (isMainFrame) mainDocumentResponseHeaders else emptyMap()
        return Answer.File(path, mimeFor(path), headers)
    }

    private fun notFound() = WebResourceResponse("text/plain", "UTF-8", ByteArrayInputStream("Not Found".toByteArray())).apply {
        setStatusCodeAndReasonPhrase(404, "Not Found")
    }
}
