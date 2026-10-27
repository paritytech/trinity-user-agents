package io.paritytech.polkadotapp.feature_chats_impl.domain.usecase

import io.mockk.coEvery
import io.mockk.coVerify
import io.mockk.every
import io.mockk.mockk
import io.paritytech.polkadotapp.common.data.time.TimeProvider
import io.paritytech.polkadotapp.common.domain.model.intoAccountId
import io.paritytech.polkadotapp.common.domain.model.requireX25519PublicKey
import io.paritytech.polkadotapp.feature_account_api.domain.model.SharedSecretDerivationDomain
import io.paritytech.polkadotapp.feature_chats_api.domain.model.Contact
import io.paritytech.polkadotapp.feature_chats_api.domain.profile.ContactAvatarFormat
import io.paritytech.polkadotapp.feature_chats_api.domain.profile.ContactPublicProfile
import io.paritytech.polkadotapp.feature_chats_api.domain.profile.ResolvedContactAvatar
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ContactsRepository
import io.paritytech.polkadotapp.feature_chats_impl.data.storage.ContactAvatarFileStorage
import io.paritytech.polkadotapp.feature_chats_impl.domain.profile.CompoundContactPublicProfileSource
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertTrue
import org.junit.Test
import kotlin.time.Duration.Companion.minutes
import kotlin.time.Instant

class SyncContactProfileUseCaseTest {
    private val now = Instant.fromEpochMilliseconds(1_800_000_000_000)
    private val accountId = ByteArray(32) { 7 }.intoAccountId()
    private val contactsRepository = mockk<ContactsRepository>(relaxUnitFun = true)
    private val profileSource = mockk<CompoundContactPublicProfileSource>()
    private val avatarFiles = mockk<ContactAvatarFileStorage>(relaxUnitFun = true)
    private val timeProvider = mockk<TimeProvider> { every { now() } returns now }
    private val useCase = SyncContactProfileUseCase(contactsRepository, profileSource, avatarFiles, timeProvider)

    private val webp = ResolvedContactAvatar(cid = "bafkcid1", format = ContactAvatarFormat.WEBP, bytes = byteArrayOf(1, 2, 3))

    @Test
    fun `a contact with no username is never looked up`() = runTest {
        val result = useCase.sync(contact(username = null), force = false)

        assertTrue(result.isSuccess)
        coVerify(exactly = 0) { profileSource.resolve(any(), any()) }
    }

    @Test
    fun `a contact checked within the hour is not looked up unless forced`() = runTest {
        val recent = contact(checkedAt = now - 30.minutes)
        coEvery { profileSource.resolve("alice", true) } returns Result.success(ContactPublicProfile.Absent)

        useCase.sync(recent, force = false)
        coVerify(exactly = 0) { profileSource.resolve(any(), any()) }

        useCase.sync(recent, force = true)
        coVerify(exactly = 1) { profileSource.resolve("alice", true) }
    }

    @Test
    fun `a new avatar is written and the row points at it`() = runTest {
        val stale = contact(checkedAt = now - PROFILE_CHECK_INTERVAL)
        coEvery { profileSource.resolve("alice", false) } returns Result.success(ContactPublicProfile.Found(webp, name = "Alice"))
        coEvery { avatarFiles.write(accountId, webp) } returns "content://avatars/alice-bafkcid1.webp"

        val result = useCase.sync(stale, force = false)

        assertTrue(result.isSuccess)
        coVerify { contactsRepository.updateContactProfile(accountId, "content://avatars/alice-bafkcid1.webp", "bafkcid1", now) }
        coVerify { avatarFiles.delete(accountId, keepCid = "bafkcid1") }
    }

    @Test
    fun `an unchanged avatar only moves the check stamp`() = runTest {
        val current = contact(checkedAt = null, avatarCid = "bafkcid1")
        coEvery { profileSource.resolve("alice", false) } returns Result.success(ContactPublicProfile.Found(webp, name = null))

        useCase.sync(current, force = false)

        coVerify { contactsRepository.updateContactProfileCheckedAt(accountId, now) }
        coVerify(exactly = 0) { avatarFiles.write(any(), any()) }
        coVerify(exactly = 0) { contactsRepository.updateContactProfile(any(), any(), any(), any()) }
    }

    @Test
    fun `a taken-down profile clears the stored avatar`() = runTest {
        val current = contact(checkedAt = null, avatarCid = "bafkcid1")
        coEvery { profileSource.resolve("alice", false) } returns Result.success(ContactPublicProfile.Absent)

        useCase.sync(current, force = false)

        coVerify { avatarFiles.delete(accountId, keepCid = null) }
        coVerify { contactsRepository.updateContactProfile(accountId, null, null, now) }
    }

    @Test
    fun `a profile without an avatar clears a stored one and stamps a contact that had none`() = runTest {
        val noAvatar = ContactPublicProfile.Found(avatar = null, name = "Alice")
        coEvery { profileSource.resolve("alice", false) } returns Result.success(noAvatar)

        useCase.sync(contact(checkedAt = null, avatarCid = "bafkcid1"), force = false)
        coVerify { contactsRepository.updateContactProfile(accountId, null, null, now) }

        useCase.sync(contact(checkedAt = null), force = false)
        coVerify { contactsRepository.updateContactProfileCheckedAt(accountId, now) }
    }

    @Test
    fun `a failed lookup changes nothing, not even the check stamp`() = runTest {
        coEvery { profileSource.resolve("alice", false) } returns Result.failure(IllegalStateException("chain unreachable"))

        val result = useCase.sync(contact(checkedAt = null, avatarCid = "bafkcid1"), force = false)

        assertTrue(result.isFailure)
        coVerify(exactly = 0) { contactsRepository.updateContactProfile(any(), any(), any(), any()) }
        coVerify(exactly = 0) { contactsRepository.updateContactProfileCheckedAt(any(), any()) }
        coVerify(exactly = 0) { avatarFiles.delete(any(), any()) }
    }

    private fun contact(username: String? = "alice", checkedAt: Instant? = null, avatarCid: String? = null) = Contact(
        accountId = accountId,
        username = username,
        chatKey = ByteArray(32) { 2 }.requireX25519PublicKey(),
        ourMetaAccountId = 1,
        avatarUrl = null,
        sharedSecretDerivationDomain = SharedSecretDerivationDomain.CHAT,
        addedAt = Instant.fromEpochMilliseconds(0),
        profileAvatarUri = avatarCid?.let { "content://avatars/alice-$it.webp" },
        profileAvatarCid = avatarCid,
        profileCheckedAt = checkedAt,
    )
}
