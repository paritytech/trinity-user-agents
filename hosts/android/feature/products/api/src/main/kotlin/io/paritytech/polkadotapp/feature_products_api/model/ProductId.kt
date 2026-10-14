package io.paritytech.polkadotapp.feature_products_api.model

import android.net.Uri
import androidx.core.net.toUri
import io.paritytech.polkadotapp.common.utils.Urls
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatExtensionId
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTld
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsUtils

/**
 * A product's dotNS identity — what it is persisted under and what product-scoped permissions key
 * off. Always kind-agnostic: an executable subname such as `app.coinflip.dot` parses to
 * `coinflip.dot`, so no caller can key anything off one. Where an executable is *served* from is
 * [ExecutableHost].
 */
// Cannot be value class since it is used in assisted factories (e.g. ProductsScriptExecutorFactory)
// and dagger's assisted factories cannot accept value classes as arguments due to mangling
@ConsistentCopyVisibility
data class ProductId private constructor(val value: String) {
    companion object {
        /**
         * Construct ProductId from the product uri, which must be a dotNS domain of the active
         * network as per [DotNsUtils.normalize] — its `.dot.li` / `.paseo.li` mirrors included.
         */
        fun fromUrl(uri: Uri, tld: DotNsTld): Result<ProductId> {
            val host = DotNsUtils.normalize(uri, tld)?.host
                ?: return Result.failure(IllegalArgumentException("Not a $tld domain: $uri"))

            return fromString(host, tld)
        }

        /**
         * Construct ProductId from its bare `<name><tld>` form, where `<name>` may itself contain
         * subdomains. Example: `coinflip.dot`, `arena.coinflip.dot`.
         */
        fun fromString(value: String, tld: DotNsTld): Result<ProductId> {
            // dotNS is case-insensitive, name-hashing is byte-exact.
            val name = value.lowercase()

            return if (productIdPattern(tld).matches(name)) {
                Result.success(ProductId(name.dropExecutableLabel()))
            } else {
                Result.failure(IllegalArgumentException("Not a $tld product id: $value"))
            }
        }

        /**
         * Reconstruct ProductId from a stored value (e.g. database).
         * Trusts that the value is already a valid dotNS domain.
         */
        fun fromStoredValue(value: String): ProductId = ProductId(value)

        /**
         * `app.coinflip.dot` names an executable of `coinflip.dot`, not a product of its own, so
         * the label is dropped — unless dropping it would leave the bare TLD, since `app.dot` is a
         * legitimate product.
         */
        private fun String.dropExecutableLabel(): String {
            val label = ExecutableKind.entries
                .map { "${it.manifestKind}." }
                .firstOrNull(::startsWith)
                ?: return this
            val remainder = removePrefix(label)

            return if (remainder.contains('.')) remainder else this
        }

        private fun productIdPattern(tld: DotNsTld): Regex {
            return Regex("""([a-z0-9-]+\.)+""" + Regex.escape(tld.value))
        }
    }

    override fun toString(): String {
        return value
    }
}

fun ProductId.toChatExtensionId(): ChatExtensionId {
    return "ProductBot_$value"
}

fun ProductId.toUrl() = Urls.ensureHasProtocolOrHttps(value)

fun ProductId.toUri() = toUrl().toUri()
