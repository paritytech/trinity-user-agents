package io.paritytech.polkadotapp.feature_products_impl.domain.product

import io.paritytech.polkadotapp.feature_products_api.model.ExecutableHost
import uniffi.truapi.ExecutableKind
import uniffi.truapi.ProductId
import uniffi.truapi.productIdExecutable

// Manifest dotNS conventions: text-record keys and the `<kind>.<base>` subname layout.
internal object ProductManifest {
    const val ROOT_RECORD_KEY = "manifest"
    const val EXECUTABLE_RECORD_KEY = "executable"

    // hostOf(coinflip.dot, APP) == app.coinflip.dot. The inverse lives in productIdFromHost.
    fun hostOf(productId: ProductId, kind: ExecutableKind): ExecutableHost =
        ExecutableHost(productIdExecutable(productId, kind).value)
}
