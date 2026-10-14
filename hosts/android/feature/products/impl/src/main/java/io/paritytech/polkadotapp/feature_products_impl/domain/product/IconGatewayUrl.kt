package io.paritytech.polkadotapp.feature_products_impl.domain.product

import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_products_api.model.ProductIcon
import io.paritytech.polkadotapp.tools_ipfs_api.IpfsContentLookup
import io.paritytech.polkadotapp.tools_ipfs_api.getIpfsLinkFor

/** Gateway URL the icon's bytes can be loaded from; null when no gateway is configured. */
internal suspend fun IpfsContentLookup.gatewayUrlOf(icon: ProductIcon): String? {
    return getIpfsLinkFor(icon.cid)
        .logFailure("Failed to resolve gateway url for product icon cid ${icon.cid}")
        .getOrNull()
}
