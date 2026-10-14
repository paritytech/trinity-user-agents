package io.paritytech.polkadotapp.feature_dotns_api.presentation

/**
 * Maps a requested dotNS host to the host whose archive actually serves it, so a canonical origin
 * can stay in the WebView URL — and so in identity and permission derivation — while content comes
 * from elsewhere. Identity unless something in the graph knows better.
 */
fun interface DotNsServingHostResolver {
    suspend fun servingHostFor(requestHost: String): String

    companion object {
        val Identity = DotNsServingHostResolver { it }
    }
}
