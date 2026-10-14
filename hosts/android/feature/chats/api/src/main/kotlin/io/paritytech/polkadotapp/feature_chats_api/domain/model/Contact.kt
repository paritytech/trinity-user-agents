package io.paritytech.polkadotapp.feature_chats_api.domain.model

import io.paritytech.polkadotapp.common.data.os.OperatingSystem
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.X25519PublicKey
import io.paritytech.polkadotapp.common.utils.Identifiable
import io.paritytech.polkadotapp.feature_account_api.domain.model.SharedSecretDerivationDomain
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatPushId
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatPushToken
import kotlin.time.Instant

data class Contact(
    val accountId: AccountId,
    val username: String?,
    val chatKey: X25519PublicKey,
    val ourMetaAccountId: Long,
    val avatarUrl: String?,
    val sharedSecretDerivationDomain: SharedSecretDerivationDomain,
    val pin: String? = null,
    val pushId: ChatPushId? = null,
    val pushToken: ChatPushToken? = null,
    val voipPushToken: ChatPushToken? = null,
    val lastSharedPushToken: String? = null,
    val operatingSystem: OperatingSystem = OperatingSystem.UNKNOWN,
    val isPeerLeft: Boolean = false,
    val isBlocked: Boolean = false,
    val origin: ContactOrigin = ContactOrigins.CONTACT_CHAT,
    val pendingChatRequestId: ChatRequestId? = null,
    val pendingDevicesFanOut: Boolean = true,
    val addedAt: Instant,
    val establishedAt: Instant? = null,
) : Identifiable {
    override val identifier = accountId.value.contentToString()
}

fun Contact.pendingChatRequestIdOrThrow(): ChatRequestId {
    return requireNotNull(pendingChatRequestId) {
        "Pending chat request is expected but was not present for contact $username ($accountId)"
    }
}

fun Contact.hasPendingChatRequest(): Boolean = pendingChatRequestId != null

fun Contact.hasEstablishedChat(): Boolean = pendingChatRequestId == null
