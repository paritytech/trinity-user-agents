package io.paritytech.polkadotapp.feature_chats_impl.domain.usecase

import io.paritytech.polkadotapp.common.data.time.TimeProvider
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.runCancellableCatching
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Contact
import io.paritytech.polkadotapp.feature_chats_api.domain.profile.ContactPublicProfile
import io.paritytech.polkadotapp.feature_chats_api.domain.profile.ResolvedContactAvatar
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ContactsRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.storage.ContactAvatarFileStorage
import io.paritytech.polkadotapp.feature_chats_impl.domain.profile.CompoundContactPublicProfileSource
import javax.inject.Inject
import kotlin.time.Duration.Companion.hours
import kotlin.time.Instant

/** A contact with a name is due a check when it never had one, or its last one is older than [PROFILE_CHECK_INTERVAL]. */
internal fun Contact.isProfileCheckDue(now: Instant): Boolean {
    if (username == null) return false
    val checkedAt = profileCheckedAt ?: return true
    return now - checkedAt >= PROFILE_CHECK_INTERVAL
}

internal val PROFILE_CHECK_INTERVAL = 1.hours

class SyncContactProfileUseCase @Inject constructor(
    private val contactsRepository: ContactsRepository,
    private val profileSource: CompoundContactPublicProfileSource,
    private val avatarFiles: ContactAvatarFileStorage,
    private val timeProvider: TimeProvider,
) {
    /**
     * Bring the stored copy of [contact]'s profile avatar in line with what is published. A failure
     * leaves the row untouched, including its check stamp, so the next trigger asks again.
     */
    suspend fun sync(contact: Contact, force: Boolean): Result<Unit> {
        val username = contact.username ?: return Result.success(Unit)
        val now = timeProvider.now()
        if (!force && !contact.isProfileCheckDue(now)) return Result.success(Unit)

        return profileSource.resolve(username, force)
            .flatMap { profile -> runCancellableCatching { apply(contact, profile, now) } }
    }

    private suspend fun apply(contact: Contact, profile: ContactPublicProfile, now: Instant) {
        val avatar = (profile as? ContactPublicProfile.Found)?.avatar
        when {
            avatar == null -> clear(contact, now)
            avatar.cid == contact.profileAvatarCid -> contactsRepository.updateContactProfileCheckedAt(contact.accountId, now)
            else -> replace(contact, avatar, now)
        }
    }

    private suspend fun clear(contact: Contact, now: Instant) {
        if (contact.profileAvatarCid == null) {
            contactsRepository.updateContactProfileCheckedAt(contact.accountId, now)
            return
        }
        avatarFiles.delete(contact.accountId, keepCid = null)
        contactsRepository.updateContactProfile(contact.accountId, avatarUri = null, avatarCid = null, checkedAt = now)
    }

    private suspend fun replace(contact: Contact, avatar: ResolvedContactAvatar, now: Instant) {
        val uri = avatarFiles.write(contact.accountId, avatar)
        contactsRepository.updateContactProfile(contact.accountId, avatarUri = uri, avatarCid = avatar.cid, checkedAt = now)
        avatarFiles.delete(contact.accountId, keepCid = avatar.cid)
    }
}
