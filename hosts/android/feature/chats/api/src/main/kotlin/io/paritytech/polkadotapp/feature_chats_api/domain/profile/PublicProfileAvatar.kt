package io.paritytech.polkadotapp.feature_chats_api.domain.profile

const val PUBLIC_PROFILE_AVATAR_SCHEME = "public-profile"

/** The image model of the avatar [username]'s public profile names, resolved by the core only once it is drawn. */
fun publicProfileAvatarUri(username: String): String = "$PUBLIC_PROFILE_AVATAR_SCHEME:$username"

/** Whether [uri] is a [publicProfileAvatarUri], whose profile record the core can reread. */
fun isPublicProfileAvatarUri(uri: String): Boolean = uri.startsWith("$PUBLIC_PROFILE_AVATAR_SCHEME:")
