package io.paritytech.polkadotapp.feature_products_impl.presentation.deeplink

import io.paritytech.polkadotapp.common.presentation.deeplink.DeepLinkHandler

/** The host every Pocket link names. No product can take it: a dotNS name always ends in a TLD. */
private const val POCKET_HOST = "pocket"

/** A link to the Pocket, `polkadotapp://pocket…`, from a product's `navigateTo` or from outside the app alike. */
internal fun isPocketLink(scheme: String?, host: String?): Boolean =
    scheme == DeepLinkHandler.APP_SCHEME && host == POCKET_HOST
