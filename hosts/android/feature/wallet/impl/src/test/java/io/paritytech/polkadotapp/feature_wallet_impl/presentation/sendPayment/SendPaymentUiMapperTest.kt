package io.paritytech.polkadotapp.feature_wallet_impl.presentation.sendPayment

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.domain.model.intoAccountId
import io.paritytech.polkadotapp.common.presentation.search.RemoteSearchPhase
import io.paritytech.polkadotapp.feature_account_api.presentation.address.model.AddressCandidates
import io.paritytech.polkadotapp.feature_account_api.presentation.address.model.ExtractedAddress
import io.paritytech.polkadotapp.feature_account_api.presentation.address.model.ExtractedAddressesCategory
import io.paritytech.polkadotapp.feature_account_api.presentation.address.model.ExtractedAddressesSection
import org.junit.Assert.assertEquals
import org.junit.Test
import io.paritytech.polkadotapp.common.R as RCommon

class SendPaymentUiMapperTest {
    @Test
    fun `sections keep their order and show each person only once`() {
        val results = candidates(
            recent = listOf(address("alice.01"), address("bob.02")),
            contacts = listOf(address("alice.01"), address("carol.03")),
            remote = RemoteSearchPhase.Loaded(listOf(address("alice.01"), address("dave.04"), address("carol.03"))),
        ).toSearchResults(hiddenAccountIds = emptySet())

        assertEquals(
            listOf(
                RCommon.string.search_section_recent to listOf("alice.01", "bob.02"),
                RCommon.string.search_section_contacts to listOf("carol.03"),
                RCommon.string.search_section_all_users to listOf("dave.04"),
            ),
            results.shownSections()
        )
    }

    @Test
    fun `hidden accounts are left out of every section`() {
        val results = candidates(
            recent = listOf(address("alice.01"), address("bob.02")),
            contacts = listOf(address("carol.03")),
            remote = RemoteSearchPhase.Loaded(listOf(address("carol.03"), address("dave.04"))),
        ).toSearchResults(hiddenAccountIds = setOf(accountId("bob.02"), accountId("carol.03")))

        assertEquals(
            listOf(
                RCommon.string.search_section_recent to listOf("alice.01"),
                RCommon.string.search_section_all_users to listOf("dave.04"),
            ),
            results.shownSections()
        )
    }

    @Test
    fun `all users are sorted by name`() {
        val results = candidates(
            remote = RemoteSearchPhase.Loaded(listOf(address("zed.01"), address("Amy.02"), address("bob.03"))),
        ).toSearchResults(hiddenAccountIds = emptySet())

        assertEquals(
            listOf(RCommon.string.search_section_all_users to listOf("Amy.02", "bob.03", "zed.01")),
            results.shownSections()
        )
    }

    @Test
    fun `local matches stay on screen while the lookup is late`() {
        val results = candidates(
            recent = listOf(address("alice.01")),
            remote = RemoteSearchPhase.Pending(emptyList(), loaderDue = true),
        ).toSearchResults(hiddenAccountIds = emptySet())

        assertEquals(
            listOf(RCommon.string.search_section_recent to listOf("alice.01")),
            results.shownSections()
        )
    }

    @Test
    fun `without any match the pending lookup waits and then shows the loader`() {
        val waiting = candidates(remote = RemoteSearchPhase.Pending(emptyList(), loaderDue = false))
        val late = candidates(remote = RemoteSearchPhase.Pending(emptyList(), loaderDue = true))

        assertEquals(PaymentSearchResults.Waiting, waiting.toSearchResults(hiddenAccountIds = emptySet()))
        assertEquals(PaymentSearchResults.Loading, late.toSearchResults(hiddenAccountIds = emptySet()))
    }

    @Test
    fun `without any match a finished lookup is empty`() {
        val loaded = candidates(remote = RemoteSearchPhase.Loaded(emptyList()))
        val failed = candidates(remote = RemoteSearchPhase.Failed)

        assertEquals(PaymentSearchResults.Empty, loaded.toSearchResults(hiddenAccountIds = emptySet()))
        assertEquals(PaymentSearchResults.Empty, failed.toSearchResults(hiddenAccountIds = emptySet()))
    }

    private fun PaymentSearchResults.shownSections(): List<Pair<Int, List<String>>> {
        val sections = (this as PaymentSearchResults.Sections).sections

        return sections.map { section -> section.titleRes to section.items.map { it.extractedAddress.display } }
    }

    private fun candidates(
        recent: List<ExtractedAddress> = emptyList(),
        contacts: List<ExtractedAddress> = emptyList(),
        remote: RemoteSearchPhase<ExtractedAddress>,
    ) = AddressCandidates(
        query = "a",
        local = listOf(
            ExtractedAddressesSection(ExtractedAddressesCategory.Custom(RCommon.string.search_section_recent), recent),
            ExtractedAddressesSection(ExtractedAddressesCategory.Custom(RCommon.string.search_section_contacts), contacts),
        ),
        remote = remote,
    )

    private fun address(username: String) = ExtractedAddress(
        display = username,
        type = ExtractedAddress.DisplayType.USERNAME,
        accountId = accountId(username),
    )

    private fun accountId(username: String): AccountId = username.lowercase().encodeToByteArray().intoAccountId()
}
