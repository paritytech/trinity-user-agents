package io.paritytech.polkadotapp.feature_products_api.model.derivation

import io.paritytech.polkadotapp.feature_account_api.domain.derivation.DerivationIndex32
import io.paritytech.polkadotapp.feature_products_api.model.ProductId

private const val PRODUCT_NAMESPACE = "product"

/**
 * `//product//{productId}` — the hard junction separating a product's subtree from the root keypair's
 * other derivations. Leaking this key exposes exactly that product's accounts and nothing above them.
 */
fun productSubtreePath(productId: ProductId): String = "//$PRODUCT_NAMESPACE//${productId.value}"

/**
 * `//product//{productId}/{index}` — soft below the product boundary, since anyone holding a child
 * secret already holds the product-root secret.
 */
fun productAccountPath(productId: ProductId, index: DerivationIndex32): String =
    "${productSubtreePath(productId)}/${index.asPathSegment()}"

/**
 * `//{productId}//{index}` in the ring-VRF tree. Hard-only, mirroring the product account path shape.
 */
fun ringVrfPath(productId: ProductId, index: DerivationIndex32): String =
    "//${productId.value}//${index.asPathSegment()}"
