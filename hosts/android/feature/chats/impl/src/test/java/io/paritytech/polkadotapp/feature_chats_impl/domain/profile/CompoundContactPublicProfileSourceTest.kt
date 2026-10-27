package io.paritytech.polkadotapp.feature_chats_impl.domain.profile

import io.mockk.coEvery
import io.mockk.mockk
import io.paritytech.polkadotapp.feature_chats_api.domain.profile.ContactPublicProfile
import io.paritytech.polkadotapp.feature_chats_api.domain.profile.ContactPublicProfileSource
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test

class CompoundContactPublicProfileSourceTest {
    private val found = ContactPublicProfile.Found(avatar = null, name = "Alice")

    @Test
    fun `with no source a profile is absent, not an error`() = runTest {
        val compound = CompoundContactPublicProfileSource(emptySet())

        assertEquals(Result.success(ContactPublicProfile.Absent), compound.resolve("alice", false))
    }

    @Test
    fun `the first source that finds a profile answers`() = runTest {
        val absent = source(Result.success(ContactPublicProfile.Absent))
        val finding = source(Result.success(found))
        val compound = CompoundContactPublicProfileSource(linkedSetOf(absent, finding))

        assertSame(found, compound.resolve("alice", false).getOrNull())
    }

    @Test
    fun `a failure is kept only when nothing was found`() = runTest {
        val failing = source(Result.failure(IllegalStateException("chain unreachable")))
        val absent = source(Result.success(ContactPublicProfile.Absent))
        val finding = source(Result.success(found))

        assertTrue(CompoundContactPublicProfileSource(linkedSetOf(failing, absent)).resolve("alice", false).isFailure)
        assertSame(found, CompoundContactPublicProfileSource(linkedSetOf(failing, finding)).resolve("alice", false).getOrNull())
    }

    private fun source(answer: Result<ContactPublicProfile>) = mockk<ContactPublicProfileSource> {
        coEvery { resolve("alice", false) } returns answer
    }
}
