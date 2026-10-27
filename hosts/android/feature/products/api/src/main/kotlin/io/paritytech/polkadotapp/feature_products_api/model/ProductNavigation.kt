package io.paritytech.polkadotapp.feature_products_api.model

import android.net.Uri
import androidx.core.net.toUri
import io.paritytech.polkadotapp.common.utils.Urls
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatExtensionId
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsUtils
import uniffi.truapi.ProductId

/** Resolve a product link on the active network to its owning product. */
fun productIdFromUrl(uri: Uri, tld: DotNsTld): Result<ProductId> {
    val host = DotNsUtils.normalize(uri, tld)?.host
        ?: return Result.failure(IllegalArgumentException("Not a $tld domain: $uri"))
    return productIdFromHost(host, tld)
}

/** Executable URLs share the owning product's navigation and permission state. */
fun productIdFromHost(host: String, tld: DotNsTld): Result<ProductId> = runCatching {
    val identifier = ProductId.parse(host)
    require(identifier.value.endsWith(tld.suffix)) { "Not a $tld product id: $host" }
    val executable = ExecutableKind.entries.firstOrNull { identifier.value.startsWith("${it.manifestKind}.") }
    val owner = executable?.let { identifier.value.removePrefix("${it.manifestKind}.") }
    if (owner != null && owner.contains('.')) ProductId.parse(owner) else identifier
}

fun ProductId.toChatExtensionId(): ChatExtensionId = "ProductBot_$value"

fun ProductId.toUrl() = Urls.ensureHasProtocolOrHttps(value)

fun ProductId.toUri() = toUrl().toUri()
