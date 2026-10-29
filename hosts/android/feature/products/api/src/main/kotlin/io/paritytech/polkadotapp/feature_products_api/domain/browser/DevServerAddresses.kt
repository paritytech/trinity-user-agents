package io.paritytech.polkadotapp.feature_products_api.domain.browser

/** Addresses of development servers on the developer's machine, which a debug build opens as products. */
interface DevServerAddresses {
    /**
     * The address to open for [address], such as `http://192.168.1.59:3000/` for `192.168.1.59:3000`, with any
     * `product_id` query kept so a reopened tab runs as the same product. Fails with the core's reason when
     * [address] names no development server or an invalid product id, or when this build opens none.
     */
    fun urlOf(address: String): Result<String>
}
