package io.paritytech.polkadotapp.feature_videogame_impl.deeplink

import android.net.Uri
import io.paritytech.polkadotapp.common.presentation.deeplink.DeepLinkHandler
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import javax.inject.Inject

const val WEEKLY_GAME_HOST = "weeklygame"
const val WAITING_ROOM_PATH = "waitingroom"
const val PRODUCT_GAME_PATH = "product"

fun Uri.isProductGameLink(): Boolean = pathSegments.firstOrNull() == PRODUCT_GAME_PATH

class VideoGameDeeplinkMapper @Inject constructor() {
    fun toWeeklyGameBotDeeplink(): Uri {
        return Uri.Builder()
            .scheme(DeepLinkHandler.APP_SCHEME)
            .authority(WEEKLY_GAME_HOST)
            .build()
    }

    fun toWaitingRoomDeeplink(): Uri {
        return Uri.Builder()
            .scheme(DeepLinkHandler.APP_SCHEME)
            .authority(WEEKLY_GAME_HOST)
            .appendPath(WAITING_ROOM_PATH)
            .build()
    }

    fun toProductGameDeeplink(productId: ProductId): Uri {
        return Uri.Builder()
            .scheme(DeepLinkHandler.APP_SCHEME)
            .authority(WEEKLY_GAME_HOST)
            .appendPath(PRODUCT_GAME_PATH)
            .appendPath(productId.value)
            .build()
    }
}
