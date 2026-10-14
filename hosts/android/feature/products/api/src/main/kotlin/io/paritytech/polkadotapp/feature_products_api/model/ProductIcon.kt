package io.paritytech.polkadotapp.feature_products_api.model

import io.paritytech.polkadotapp.tools_ipfs_api.Cid

data class ProductIcon(
    val cid: Cid,
    val format: Format,
) {
    enum class Format {
        JPEG, PNG
    }
}
