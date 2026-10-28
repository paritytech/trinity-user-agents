package io.paritytech.polkadotapp.feature_products_api.model

import android.net.Uri
import androidx.core.net.toUri
import io.paritytech.polkadotapp.common.utils.Urls
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatExtensionId
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import uniffi.truapi.ProductId
import uniffi.truapi.parseProductLink
import uniffi.truapi.productIdMatchesNetwork

/** Resolve a product link on the active network to its owning product. */
fun productIdFromUrl(uri: Uri, tld: DotNsTld): Result<ProductId> {
    val link = parseProductLink(uri.toString(), tld.value)
        ?: return Result.failure(IllegalArgumentException("Not a $tld domain: $uri"))
    return link.identifier.owner()
}

/** Executable URLs share the owning product's navigation and permission state. */
fun productIdFromHost(host: String, tld: DotNsTld): Result<ProductId> = ProductId.parse(host).flatMap { identifier ->
    if (productIdMatchesNetwork(identifier, tld.value)) {
        identifier.owner()
    } else {
        Result.failure(IllegalArgumentException("Not a $tld product id: $host"))
    }
}

fun ProductId.toChatExtensionId(): ChatExtensionId = "ProductBot_$value"

fun ProductId.toUrl() = Urls.ensureHasProtocolOrHttps(value)

fun ProductId.toUri() = toUrl().toUri()
