package io.paritytech.polkadotapp.common.data.image.loadables

import android.graphics.drawable.Drawable
import io.paritytech.polkadotapp.common.data.image.fetchers.JSImageLoaderWebView

interface ImageLoadable

interface UrlImageLoadable : ImageLoadable {
    suspend fun getUrl(): String?
}

interface JsImageLoadable : ImageLoadable {
    suspend fun getDrawable(webView: JSImageLoaderWebView): Drawable?
    fun getCacheKey(): String
}
