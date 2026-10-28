package io.paritytech.polkadotapp.feature_chats_api.domain.profile

/**
 * A contact's public profile, published under their dotNS name and resolved by the core.
 *
 * A feature that can reach the core contributes an implementation; chats reads through the set
 * of them, so chats works with none.
 */
interface ContactPublicProfileSource {
    /**
     * The profile under [username] (`alice`, `alice.42`), or [ContactPublicProfile.Absent] when the
     * name has none. A failure means nothing is known: the caller keeps what it has and asks again
     * later. [force] reads the chain regardless of the core's cache.
     */
    suspend fun resolve(username: String, force: Boolean): Result<ContactPublicProfile>
}

sealed interface ContactPublicProfile {
    class Found(val avatar: ResolvedContactAvatar?, val name: String?) : ContactPublicProfile

    data object Absent : ContactPublicProfile
}

class ResolvedContactAvatar(val cid: String, val format: ContactAvatarFormat, val bytes: ByteArray)

enum class ContactAvatarFormat(val extension: String) {
    PNG("png"),
    JPEG("jpg"),
    WEBP("webp"),
}
