package io.paritytech.polkadotapp.feature_products_impl.domain.profile

import coil.intercept.Interceptor
import coil.request.ErrorResult
import coil.request.ImageResult
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.design.components.avatar.AVATAR_REFRESH_PARAMETER
import io.paritytech.polkadotapp.design.components.avatar.AvatarNotFoundException
import io.paritytech.polkadotapp.feature_chats_api.domain.profile.isPublicProfileAvatarUri
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIHostRuntimeProvider

internal class PublicProfileAvatarInterceptor(
    private val runtimeProvider: TrUAPIHostRuntimeProvider,
) : Interceptor {
    override suspend fun intercept(chain: Interceptor.Chain): ImageResult {
        val request = chain.request
        val uri = request.data.toString()
        if (!isPublicProfileAvatarUri(uri)) return chain.proceed(request)

        val username = uri.substringAfter(':')
        val force = request.parameters.value<Boolean>(AVATAR_REFRESH_PARAMETER) == true
        val profile = runtimeProvider.runtime()
            .flatMap { runtime -> runCancellableCatching { runtime.resolvePublicProfile(username, force) } }
            .logFailure("Public profile of $username")
        val avatar = profile.getOrNull()?.avatar
            ?: return ErrorResult(
                drawable = request.error,
                request = request,
                throwable = profile.exceptionOrNull() ?: AvatarNotFoundException(),
            )

        return chain.proceed(
            request.newBuilder()
                .data(avatar)
                .diskCacheKey(avatar.cid)
                .build()
        )
    }
}
