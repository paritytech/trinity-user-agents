package io.paritytech.polkadotapp.feature_products_impl.domain.scriptExecutor

import androidx.core.net.toUri

/** Splits a worker's flat script URL into the WebView origin and the entry module relative to it. */
data class WorkerScript(
    val baseUrl: String,
    val entrypoint: String,
) {
    companion object {
        fun of(scriptUrl: String): WorkerScript {
            val uri = scriptUrl.toUri()
            val scheme = uri.scheme ?: "https"
            val host = uri.host.orEmpty()
            val port = uri.port.takeIf { it != -1 }?.let { ":$it" } ?: ""
            return WorkerScript(
                baseUrl = "$scheme://$host$port",
                entrypoint = uri.path?.removePrefix("/").orEmpty(),
            )
        }
    }
}
