package io.paritytech.polkadotapp.common.data.image.interceptors

import coil.intercept.Interceptor
import coil.request.ImageRequest
import coil.request.ImageResult
import io.paritytech.polkadotapp.common.data.image.loadables.UrlImageLoadable
import io.paritytech.polkadotapp.common.presentation.ui.images.IconModel

class ImageLoaderInterceptor : Interceptor {
    override suspend fun intercept(chain: Interceptor.Chain): ImageResult {
        val request = when (val data = chain.request.data) {
            is UrlImageLoadable -> ImageRequest.Builder(chain.request).data(data.getUrl()).build()
            is IconModel.FromDrawableRes -> ImageRequest.Builder(chain.request).data(data.res).build()
            else -> chain.request
        }

        return chain.proceed(request)
    }
}
