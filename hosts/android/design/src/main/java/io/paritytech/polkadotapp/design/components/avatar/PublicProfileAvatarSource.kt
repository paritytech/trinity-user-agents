package io.paritytech.polkadotapp.design.components.avatar

import coil.request.ImageRequest
import dagger.hilt.EntryPoint
import dagger.hilt.InstallIn
import dagger.hilt.components.SingletonComponent
import kotlinx.coroutines.flow.Flow

/** Supplies successive avatar requests; null restores the letter tile and errors preserve the current image. */
interface PublicProfileAvatarSource {
    fun observe(username: String, forceFresh: Boolean): Flow<ImageRequest?>
}

@EntryPoint
@InstallIn(SingletonComponent::class)
interface PublicProfileAvatarSourceEntryPoint {
    fun publicProfileAvatarSource(): PublicProfileAvatarSource
}
