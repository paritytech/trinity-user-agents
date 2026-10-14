package io.paritytech.polkadotapp.feature_wallet_impl.presentation.sendPayment.compose.components

import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.util.fastForEach
import io.paritytech.polkadotapp.design.components.avatar.NovaContactItem
import io.paritytech.polkadotapp.design.components.avatar.NovaContactItemType
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_account_api.presentation.address.model.ExtractedAddress
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.sendPayment.PaymentSearchResultUiModel
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.sendPayment.PaymentSearchSectionUiModel
import kotlinx.collections.immutable.ImmutableList

@Composable
internal fun SearchResult(
    sections: ImmutableList<PaymentSearchSectionUiModel>,
    onRecipientSelect: (PaymentSearchResultUiModel) -> Unit
) {
    LazyColumn(
        modifier = Modifier.fillMaxWidth()
    ) {
        sections.fastForEach { section ->
            item(key = "header_${section.key}") {
                SearchSectionHeader(title = stringResource(section.titleRes))
            }

            items(
                items = section.items,
                key = { "${section.key}_${it.extractedAddress.display}" }
            ) { uiModel ->
                val candidate = uiModel.extractedAddress
                val onClick = remember(uiModel) { { onRecipientSelect(uiModel) } }

                val itemType = when (candidate.type) {
                    ExtractedAddress.DisplayType.USERNAME -> NovaContactItemType.User
                    ExtractedAddress.DisplayType.ADDRESS -> NovaContactItemType.Address
                }
                NovaContactItem(
                    title = candidate.display,
                    type = itemType,
                    avatarModel = uiModel.avatarModel,
                    onClick = onClick
                )
            }
        }
    }
}

@Composable
private fun SearchSectionHeader(title: String) {
    NovaText(
        modifier = Modifier
            .fillMaxWidth()
            .padding(
                horizontal = PolkadotTheme.spacings.extraMedium,
                vertical = PolkadotTheme.spacings.small
            ),
        text = title,
        style = PolkadotTheme.typography.caption.medium,
        color = PolkadotTheme.colors.fg.secondary,
    )
}
