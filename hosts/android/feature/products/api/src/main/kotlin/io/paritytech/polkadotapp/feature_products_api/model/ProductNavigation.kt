package io.paritytech.polkadotapp.feature_products_api.model

import android.net.Uri
import androidx.core.net.toUri
import io.paritytech.polkadotapp.common.utils.Urls
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatExtensionId
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import uniffi.truapi.ProductId
import uniffi.truapi.parseProductLink
import uniffi.truapi.productIdMatchesNetwork
import uniffi.truapi.productIdOwner

/** Resolve a product link on the active network to its owning product. */
fun productIdFromUrl(uri: Uri, tld: DotNsTld): Result<ProductId> = runCatching {
    val link = requireNotNull(parseProductLink(uri.toString(), tld.value)) { "Not a $tld domain: $uri" }
    productIdOwner(link.identifier)
}

/** Executable URLs share the owning product's navigation and permission state. */
fun productIdFromHost(host: String, tld: DotNsTld): Result<ProductId> = ProductId.parse(host).mapCatching { identifier ->
    require(productIdMatchesNetwork(identifier, tld.value)) { "Not a $tld product id: $host" }
    productIdOwner(identifier)
}

fun ProductId.toChatExtensionId(): ChatExtensionId = "ProductBot_$value"

fun ProductId.toUrl() = Urls.ensureHasProtocolOrHttps(value)

fun ProductId.toUri() = toUrl().toUri()
