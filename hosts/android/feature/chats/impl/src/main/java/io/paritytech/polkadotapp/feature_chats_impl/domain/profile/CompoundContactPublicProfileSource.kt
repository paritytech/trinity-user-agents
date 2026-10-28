package io.paritytech.polkadotapp.feature_chats_impl.domain.profile

import io.paritytech.polkadotapp.feature_chats_api.domain.profile.ContactPublicProfile
import io.paritytech.polkadotapp.feature_chats_api.domain.profile.ContactPublicProfileSource
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class CompoundContactPublicProfileSource @Inject constructor(
    private val sources: Set<@JvmSuppressWildcards ContactPublicProfileSource>,
) : ContactPublicProfileSource {
    override suspend fun resolve(username: String, force: Boolean): Result<ContactPublicProfile> {
        var failure: Throwable? = null
        for (source in sources) {
            source.resolve(username, force).fold(
                onSuccess = { profile -> if (profile is ContactPublicProfile.Found) return Result.success(profile) },
                onFailure = { failure = it },
            )
        }
        return failure?.let { Result.failure(it) } ?: Result.success(ContactPublicProfile.Absent)
    }
}
