package io.paritytech.polkadotapp.feature_videogame_impl.deeplink

import android.net.Uri
import io.paritytech.polkadotapp.common.presentation.deeplink.DeepLinkHandler
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import javax.inject.Inject

const val WEEKLY_GAME_HOST = "weeklygame"
const val PRODUCT_GAME_PATH = "product"
const val PRODUCT_GAME_STARTS_AT_PARAM = "startsAt"

fun Uri.isProductGameLink(): Boolean = pathSegments.firstOrNull() == PRODUCT_GAME_PATH

class VideoGameDeeplinkMapper @Inject constructor() {
    fun toProductGameDeeplink(productId: ProductId, startsAtMillis: Long): Uri {
        return Uri.Builder()
            .scheme(DeepLinkHandler.APP_SCHEME)
            .authority(WEEKLY_GAME_HOST)
            .appendPath(PRODUCT_GAME_PATH)
            .appendPath(productId.value)
            .appendQueryParameter(PRODUCT_GAME_STARTS_AT_PARAM, startsAtMillis.toString())
            .build()
    }
}
