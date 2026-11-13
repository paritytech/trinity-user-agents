package io.parity.truapi

import android.security.NetworkSecurityPolicy
import uniffi.truapi_provider.ChainCloseReason
import uniffi.truapi_provider.ChainConnection
import uniffi.truapi_provider.ChainMessageListener
import uniffi.truapi_provider.ChainProvider
import uniffi.truapi_provider.ChainProviderException
import uniffi.truapi_provider.RpcNode
import java.net.URI

/** Thrown by [dialRpcNode] for a `ws://` node whose host may not be reached in the clear. */
class CleartextNotPermittedException(url: String) :
    Exception("cleartext traffic is not permitted to the host of $url")

/** Whether the app's network security config allows cleartext traffic to [host]. */
fun systemPermitsCleartext(host: String): Boolean =
    NetworkSecurityPolicy.getInstance().isCleartextTrafficPermitted(host)

/**
 * Dials [url] as the only node of the chain with [genesisHash], so nothing sent
 * on the returned connection can reach another node or the light client.
 *
 * Blocks until the node answers or fails. The provider's sockets bypass the
 * app's network security config, so a `ws://` URL is dialled only when
 * [cleartextPermitted] allows its host; a malformed URL is left for the provider
 * to reject.
 *
 * @throws CleartextNotPermittedException when [cleartextPermitted] refuses the host.
 * @throws ChainProviderException when the node cannot be reached.
 */
fun dialRpcNode(
    genesisHash: ByteArray,
    url: String,
    listener: ChainMessageListener,
    cleartextPermitted: (host: String) -> Boolean = ::systemPermitsCleartext,
): ChainConnection {
    val target = runCatching { URI(url) }.getOrNull()
    if (target?.scheme == "ws" && !cleartextPermitted(target.host.orEmpty())) {
        throw CleartextNotPermittedException(url)
    }
    return ChainProvider.withRpcNodes(listOf(RpcNode(genesisHash, url))).use { node ->
        node.connect(genesisHash, listener)
    }
}

/** The provider's reason for a failed dial, for logs. */
fun ChainProviderException.describe(): String =
    (this as? ChainProviderException.Connect)?.reason ?: javaClass.simpleName

/** Why the provider ended a connection, for logs. */
fun ChainCloseReason.describe(): String = when (this) {
    ChainCloseReason.StreamEnded -> "stream ended"
    is ChainCloseReason.ListenerFailed -> "listener failed: $reason"
}
