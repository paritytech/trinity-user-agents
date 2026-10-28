package io.paritytech.polkadotapp.feature_products_impl.domain.profile

import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_chats_api.domain.profile.ContactAvatarFormat
import io.paritytech.polkadotapp.feature_chats_api.domain.profile.ContactPublicProfile
import io.paritytech.polkadotapp.feature_chats_api.domain.profile.ContactPublicProfileSource
import io.paritytech.polkadotapp.feature_chats_api.domain.profile.ResolvedContactAvatar
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.TrUAPIHostRuntimeProvider
import uniffi.truapi.ProfileAvatarFormat
import uniffi.truapi.PublicProfileLookup
import uniffi.truapi.ResolvedAvatar
import javax.inject.Inject

class TrUAPIContactPublicProfileSource @Inject constructor(
    private val runtimeProvider: TrUAPIHostRuntimeProvider,
) : ContactPublicProfileSource {
    override suspend fun resolve(username: String, force: Boolean): Result<ContactPublicProfile> {
        return runtimeProvider.runtime()
            .flatMap { runtime -> runCancellableCatching { runtime.resolvePublicProfile(username, force) } }
            .flatMap { lookup -> lookup.toDomain() }
    }

    private fun PublicProfileLookup.toDomain(): Result<ContactPublicProfile> = when (this) {
        is PublicProfileLookup.Found -> Result.success(ContactPublicProfile.Found(avatar?.toDomain(), name))
        PublicProfileLookup.Absent -> Result.success(ContactPublicProfile.Absent)
        is PublicProfileLookup.Failed -> Result.failure(PublicProfileLookupException(reason))
    }

    private fun ResolvedAvatar.toDomain() = ResolvedContactAvatar(cid = cid, format = format.toDomain(), bytes = bytes)

    private fun ProfileAvatarFormat.toDomain() = when (this) {
        ProfileAvatarFormat.PNG -> ContactAvatarFormat.PNG
        ProfileAvatarFormat.JPEG -> ContactAvatarFormat.JPEG
        ProfileAvatarFormat.WEBP -> ContactAvatarFormat.WEBP
    }
}

class PublicProfileLookupException(reason: String) : RuntimeException("The core could not resolve the public profile: $reason")
