package io.paritytech.polkadotapp.feature_products_impl.domain.browser

import io.paritytech.polkadotapp.common.utils.FeatureOption
import io.paritytech.polkadotapp.common.utils.isDisabled
import io.paritytech.polkadotapp.feature_products_api.domain.browser.DevServerAddresses
import uniffi.truapi.DevServerProduct
import uniffi.truapi.parseDevServer
import javax.inject.Inject

/**
 * The only place a development server is recognised, and so the only gate on one: the product it names
 * runs with the core's development wildcard, or as the product its `product_id` query names.
 */
class DevServers @Inject constructor() : DevServerAddresses {
    fun productAt(url: String): DevServerProduct? = parse(url).getOrNull()

    override fun urlOf(address: String): Result<String> = parse(address).map(DevServerProduct::url)

    private fun parse(address: String): Result<DevServerProduct> =
        if (FeatureOption.DEV_SERVER_PRODUCTS.isDisabled) {
            Result.failure(IllegalStateException("This build opens no development server"))
        } else {
            runCatching { parseDevServer(address) }
        }
}
