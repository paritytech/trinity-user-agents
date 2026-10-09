package io.paritytech.polkadotapp.feature_products_impl.domain.browser

import io.paritytech.polkadotapp.common.utils.FeatureOption
import io.paritytech.polkadotapp.common.utils.isEnabled
import io.paritytech.polkadotapp.feature_products_api.domain.browser.DevServerAddresses
import uniffi.truapi.DevServerProduct
import uniffi.truapi.parseDevServer
import javax.inject.Inject

/**
 * The only place a development server is recognised, and so the only gate on one: the product it names
 * runs under a `localhost` identifier, which the core grants a development wildcard.
 */
class DevServers @Inject constructor() : DevServerAddresses {
    fun productAt(url: String): DevServerProduct? =
        if (FeatureOption.DEV_SERVER_PRODUCTS.isEnabled) parseDevServer(url) else null

    override fun originOf(address: String): String? = productAt(address)?.origin
}
