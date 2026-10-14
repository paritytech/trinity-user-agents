package io.paritytech.polkadotapp.feature_products_impl.domain.accountsProtocol.locator

import io.paritytech.polkadotapp.feature_account_api.data.repository.AccountRepository
import io.paritytech.polkadotapp.feature_members_api.data.model.RingCollectionId
import io.paritytech.polkadotapp.feature_people_api.domain.PEOPLE
import io.paritytech.polkadotapp.feature_people_api.domain.PEOPLE_LITE
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import org.mockito.Mockito.mock

class MembersRingLocatorStrategyTest {
    private val accountRepository: AccountRepository = mock(AccountRepository::class.java)
    private val otherCollection: RingCollectionId = RingCollectionId.paddedString("some:other/collection")

    @Test
    fun `full locator applies to the PEOPLE collection only`() {
        val locator = FullPeopleRingLocator(accountRepository)

        assertTrue(locator.appliesTo(RingCollectionId.PEOPLE))
        assertFalse(locator.appliesTo(RingCollectionId.PEOPLE_LITE))
        assertFalse(locator.appliesTo(null))
        assertEquals(RingCollectionId.PEOPLE, locator.resolveCollectionId(RingCollectionId.PEOPLE))
    }

    @Test
    fun `lite locator applies to the PEOPLE_LITE collection only`() {
        val locator = LitePeopleRingLocator(accountRepository)

        assertTrue(locator.appliesTo(RingCollectionId.PEOPLE_LITE))
        assertFalse(locator.appliesTo(RingCollectionId.PEOPLE))
        assertFalse(locator.appliesTo(null))
        assertEquals(RingCollectionId.PEOPLE_LITE, locator.resolveCollectionId(RingCollectionId.PEOPLE_LITE))
    }

    @Test
    fun `fallback locator applies to any collection and keeps the requested one`() {
        val locator = FallbackPeopleRingLocator(accountRepository)

        assertTrue(locator.appliesTo(null))
        assertTrue(locator.appliesTo(RingCollectionId.PEOPLE))
        assertTrue(locator.appliesTo(otherCollection))
        assertEquals(otherCollection, locator.resolveCollectionId(otherCollection))
    }

    @Test
    fun `fallback locator defaults to the PEOPLE collection when none is requested`() {
        val locator = FallbackPeopleRingLocator(accountRepository)

        assertEquals(RingCollectionId.PEOPLE, locator.resolveCollectionId(null))
    }
}
