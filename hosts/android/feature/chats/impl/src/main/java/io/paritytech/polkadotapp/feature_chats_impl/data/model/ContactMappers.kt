package io.paritytech.polkadotapp.feature_chats_impl.data.model

import io.paritytech.polkadotapp.common.data.os.OperatingSystem
import io.paritytech.polkadotapp.common.domain.model.intoAccountId
import io.paritytech.polkadotapp.common.domain.model.requireX25519PublicKey
import io.paritytech.polkadotapp.common.domain.model.toDataByteArray
import io.paritytech.polkadotapp.database.model.ContactLocal
import io.paritytech.polkadotapp.database.model.ContactWithChatRequestLocal
import io.paritytech.polkadotapp.database.model.ContactWithRequestTimestampLocal
import io.paritytech.polkadotapp.feature_account_api.domain.model.SharedSecretDerivationDomain
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Contact
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ContactWithChatRequest
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.toDomain
import kotlin.time.Instant
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ContactWithRequestTimestamp as DomainContactWithRequestTimestamp

fun ContactLocal.toDomain(): Contact {
    return Contact(
        accountId = accountId.intoAccountId(),
        username = username,
        chatKey = chatKey.requireX25519PublicKey(),
        ourMetaAccountId = ourMetaAccountId,
        sharedSecretDerivationDomain = SharedSecretDerivationDomain(sharedSecretDerivationPath),
        pin = pin,
        pushId = pushId?.toDataByteArray(),
        pushToken = pushToken?.toDataByteArray(),
        voipPushToken = voipPushToken?.toDataByteArray(),
        lastSharedPushToken = lastSharedPushToken,
        operatingSystem = operatingSystem.toDomain(),
        isPeerLeft = isPeerLeft,
        isBlocked = isBlocked,
        avatarUrl = avatar,
        origin = origin,
        pendingChatRequestId = chatRequestId,
        pendingDevicesFanOut = pendingDevicesFanOut,
        addedAt = Instant.fromEpochMilliseconds(addedAt),
        establishedAt = establishedAt?.let(Instant::fromEpochMilliseconds),
    )
}

fun Contact.toLocal(): ContactLocal {
    return ContactLocal(
        accountId = accountId.value,
        username = username,
        chatKey = chatKey.bytes.value,
        ourMetaAccountId = ourMetaAccountId,
        sharedSecretDerivationPath = sharedSecretDerivationDomain.domain,
        avatar = avatarUrl,
        pin = pin,
        pushId = pushId?.value,
        pushToken = pushToken?.value,
        voipPushToken = voipPushToken?.value,
        lastSharedPushToken = lastSharedPushToken,
        operatingSystem = operatingSystem.toLocal(),
        isPeerLeft = isPeerLeft,
        isBlocked = isBlocked,
        origin = origin,
        chatRequestId = pendingChatRequestId,
        pendingDevicesFanOut = pendingDevicesFanOut,
        addedAt = addedAt.toEpochMilliseconds(),
        establishedAt = establishedAt?.toEpochMilliseconds(),
    )
}

fun OperatingSystem.toLocal(): ContactLocal.OperatingSystem? = when (this) {
    OperatingSystem.ANDROID -> ContactLocal.OperatingSystem.ANDROID
    OperatingSystem.IOS -> ContactLocal.OperatingSystem.IOS
    OperatingSystem.UNKNOWN -> null
}

fun ContactLocal.OperatingSystem?.toDomain() = when (this) {
    ContactLocal.OperatingSystem.ANDROID -> OperatingSystem.ANDROID
    ContactLocal.OperatingSystem.IOS -> OperatingSystem.IOS
    null -> OperatingSystem.UNKNOWN
}

fun ContactWithRequestTimestampLocal.toDomain(): DomainContactWithRequestTimestamp {
    return DomainContactWithRequestTimestamp(
        contact = contact.toDomain(),
        requestTimestamp = requestTimestamp
    )
}

fun ContactWithChatRequestLocal.toDomain(): ContactWithChatRequest {
    return ContactWithChatRequest(
        contact = contact.toDomain(),
        pendingChatRequest = chatRequest?.toDomain()
    )
}
