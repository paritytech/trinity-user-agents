package io.paritytech.polkadotapp.chains.multiNetwork.connection

import io.paritytech.polkadotapp.chains.multiNetwork.chain.model.Chain
import io.paritytech.polkadotapp.common.utils.formatNamedOrThrow
import timber.log.Timber

class ConnectionSecrets(private val secretsByName: Map<String, String>) : Map<String, String> by secretsByName {
    companion object {
        fun default(): ConnectionSecrets {
            return ConnectionSecrets(emptyMap())
        }
    }
}

fun ConnectionSecrets.saturateUrl(url: String): String? {
    return runCatching { url.formatNamedOrThrow(this) }.getOrNull()
}

fun List<Chain.Node>.saturateNodeUrls(connectionSecrets: ConnectionSecrets): List<NodeWithSaturatedUrl> {
    return mapNotNull { node ->
        val saturatedUrl =
            connectionSecrets.saturateUrl(node.unformattedUrl) ?: run {
                Timber.w("Failed to saturate url ${node.unformattedUrl} due to unknown secrets in the url")
                return@mapNotNull null
            }

        NodeWithSaturatedUrl(node, saturatedUrl)
    }
}
