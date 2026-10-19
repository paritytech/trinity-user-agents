package io.paritytech.polkadotapp.feature_products_impl.presentation.deeplink

import android.net.Uri
import io.paritytech.polkadotapp.common.presentation.deeplink.DeepLinkHandler

/** The host every Pocket link names. No product can take it: a dotNS name always ends in a TLD. */
private const val POCKET_HOST = "pocket"

/** The scheme the core reads a Pocket link under. From outside the app the link carries [DeepLinkHandler.APP_SCHEME]. */
private const val CORE_SCHEME = "polkadot"

/** A link to the Pocket: `polkadot://pocket…` as the core hands it over, or the same link under the app's own scheme. */
internal fun isPocketLink(scheme: String?, host: String?): Boolean =
    host == POCKET_HOST && (scheme == CORE_SCHEME || scheme == DeepLinkHandler.APP_SCHEME)

/** This Pocket link under [CORE_SCHEME], the only scheme the core reads it under. */
internal fun Uri.asCorePocketLink(): Uri = buildUpon().scheme(CORE_SCHEME).build()
