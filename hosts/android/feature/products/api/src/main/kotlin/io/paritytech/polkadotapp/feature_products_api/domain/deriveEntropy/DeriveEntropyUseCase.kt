package io.paritytech.polkadotapp.feature_products_api.domain.deriveEntropy

import io.paritytech.polkadotapp.feature_products_api.model.ProductId

/**
 * Entropy derivation per TrUAPI RFC-7. Layers, from outermost in:
 *
 *  1. `rootEntropySource = blake2b256_keyed(rootAccountSecret, "product-entropy-derivation")`
 *  2. `perProductEntropy = blake2b256_keyed(rootEntropySource, blake2b256(productId))`
 *  3. `requestedEntropy  = blake2b256_keyed(perProductEntropy, key)`
 *
 * [deriveEntropy] runs all three layers for a given product + caller-chosen key.
 * [deriveRootEntropySource] returns just layer 1 — used by the SSO handshake to share
 * the seed with the host so the host can derive every per-product entropy locally.
 */
interface DeriveEntropyUseCase {
    suspend fun deriveEntropy(productId: ProductId, key: ByteArray): Result<ByteArray>
    suspend fun deriveRootEntropySource(): Result<ByteArray>
}
