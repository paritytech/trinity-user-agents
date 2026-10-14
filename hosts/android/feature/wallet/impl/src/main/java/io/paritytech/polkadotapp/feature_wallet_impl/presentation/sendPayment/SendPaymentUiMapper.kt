package io.paritytech.polkadotapp.feature_wallet_impl.presentation.sendPayment

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.presentation.search.RemoteSearchPhase
import io.paritytech.polkadotapp.design.components.avatar.AvatarUiModel
import io.paritytech.polkadotapp.design.configs.colors.AvatarColorScheme
import io.paritytech.polkadotapp.feature_account_api.presentation.address.model.AddressCandidates
import io.paritytech.polkadotapp.feature_account_api.presentation.address.model.ExtractedAddress
import io.paritytech.polkadotapp.feature_account_api.presentation.address.model.ExtractedAddressesCategory
import io.paritytech.polkadotapp.feature_account_api.presentation.address.model.ExtractedAddressesSection
import kotlinx.collections.immutable.toImmutableList
import io.paritytech.polkadotapp.common.R as RCommon

internal fun AddressCandidates.toSearchResults(hiddenAccountIds: Set<AccountId>): PaymentSearchResults {
    val allUsers = ExtractedAddressesSection.general(remote.results.sortedBy { it.display.lowercase() })
    val sections = (local + allUsers).toSearchSections(hiddenAccountIds)

    return if (sections.isNotEmpty()) {
        PaymentSearchResults.Sections(sections.toImmutableList())
    } else {
        when (val remote = remote) {
            is RemoteSearchPhase.Pending -> if (remote.loaderDue) PaymentSearchResults.Loading else PaymentSearchResults.Waiting
            is RemoteSearchPhase.Loaded, RemoteSearchPhase.Failed -> PaymentSearchResults.Empty
        }
    }
}

private fun List<ExtractedAddressesSection>.toSearchSections(hiddenAccountIds: Set<AccountId>): List<PaymentSearchSectionUiModel> {
    val shownAccountIds = hiddenAccountIds.toMutableSet()

    return mapNotNull { section ->
        val sectionAddresses = section.addresses
            .distinctByAccountId()
            .filterNot { it.accountId in shownAccountIds }

        shownAccountIds += sectionAddresses.map { it.accountId }

        if (sectionAddresses.isEmpty()) {
            null
        } else {
            PaymentSearchSectionUiModel(
                key = section.category.sectionKey(),
                titleRes = section.category.titleRes(),
                items = sectionAddresses.map { it.toUi() }.toImmutableList()
            )
        }
    }
}

private fun List<ExtractedAddress>.distinctByAccountId(): List<ExtractedAddress> {
    return groupBy { it.accountId }
        .values
        .map { group -> group.find { it.type == ExtractedAddress.DisplayType.USERNAME } ?: group.first() }
}

private fun ExtractedAddressesCategory.sectionKey(): String {
    return when (this) {
        ExtractedAddressesCategory.General -> "general"
        is ExtractedAddressesCategory.Custom -> "custom_$labelRes"
    }
}

private fun ExtractedAddressesCategory.titleRes(): Int {
    return when (this) {
        ExtractedAddressesCategory.General -> RCommon.string.search_section_all_users
        is ExtractedAddressesCategory.Custom -> labelRes
    }
}

private fun ExtractedAddress.toUi(): PaymentSearchResultUiModel {
    return PaymentSearchResultUiModel(
        extractedAddress = this,
        avatarModel = AvatarUiModel.Name(
            name = display,
            colorScheme = AvatarColorScheme.from(accountId.value)
        ),
    )
}
