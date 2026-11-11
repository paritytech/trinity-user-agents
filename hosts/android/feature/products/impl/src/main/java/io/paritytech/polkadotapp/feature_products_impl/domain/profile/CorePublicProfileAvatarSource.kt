package io.paritytech.polkadotapp.feature_products_impl.domain.profile

import android.content.Context
import coil.request.ImageRequest
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.design.components.avatar.PublicProfileAvatarSource
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIHostRuntimeProvider
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.distinctUntilChangedBy
import kotlinx.coroutines.flow.emitAll
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.map
import uniffi.truapi.PublicProfileResolutionOptions
import javax.inject.Inject

class CorePublicProfileAvatarSource @Inject constructor(
    @param:ApplicationContext private val context: Context,
    private val runtimeProvider: TrUAPIHostRuntimeProvider,
) : PublicProfileAvatarSource {
    override fun observe(username: String, forceFresh: Boolean): Flow<ImageRequest?> = flow {
        val runtime = runtimeProvider.runtime().getOrThrow()
        emitAll(
            runtime.observePublicProfile(
                username,
                PublicProfileResolutionOptions(forceFresh = forceFresh, skipStale = false),
            ).distinctUntilChangedBy { it.avatar }.map { profile ->
                profile.avatar?.let { avatar ->
                    ImageRequest.Builder(context)
                        .data(avatar)
                        .diskCacheKey(avatar.cid)
                        .build()
                }
            }
        )
    }
}
