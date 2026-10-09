package io.paritytech.polkadotapp.feature_products_api.domain.browser

/** Addresses of development servers on the developer's machine, which a debug build opens as products. */
interface DevServerAddresses {
    /**
     * The origin to open for [address], such as `http://192.168.1.59:3000` for `192.168.1.59:3000`, or null
     * when it names no development server or this build opens none.
     */
    fun originOf(address: String): String?
}
