package io.paritytech.polkadotapp.feature_wallet_impl.presentation.sendPayment.compose

import android.widget.Toast
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.paritytech.polkadotapp.design.components.avatar.AvatarUiModel
import io.paritytech.polkadotapp.design.components.avatar.Mock
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonStyle
import io.paritytech.polkadotapp.design.components.button.default.PolkadotButtonSize
import io.paritytech.polkadotapp.design.components.button.default.PolkadotTextButton
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.Close
import io.paritytech.polkadotapp.design.components.icon.vectors.Scanner
import io.paritytech.polkadotapp.design.components.progress.LoadingScreenState
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.components.text.NovaTextField
import io.paritytech.polkadotapp.design.components.topbar.PolkadotTopBar
import io.paritytech.polkadotapp.design.components.topbar.TopBarTitleAlignment
import io.paritytech.polkadotapp.design.components.topbar.rememberTopBarAction
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.design.utils.collectAsEffect
import io.paritytech.polkadotapp.feature_account_api.presentation.address.model.ExtractedAddress
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.sendPayment.PaymentSearchResultUiModel
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.sendPayment.PaymentSearchResults
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.sendPayment.PaymentSearchSectionUiModel
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.sendPayment.SendPaymentContract
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.sendPayment.SendPaymentUiState
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.sendPayment.compose.components.SearchResult
import io.paritytech.polkadotapp.feature_wallet_impl.presentation.sendPayment.compose.components.SendPaymentEmptySearch
import kotlinx.collections.immutable.persistentListOf
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
fun SendPaymentScreen(contract: SendPaymentContract) {
    val uiState = contract.state.collectAsStateWithLifecycle().value

    contract.messageEvents.collectAsEffect { context, resId ->
        Toast.makeText(context, context.getString(resId), Toast.LENGTH_SHORT).show()
    }

    SendPaymentScreenInternal(
        state = uiState,
        onInputChange = contract::onInputChange,
        onRecipientSelect = contract::onRecipientSelect,
        onPasteClick = contract::onPasteClick,
        onBackClick = contract::onBackClick,
        onScannerClick = contract::onScannerClick,
    )
}

@Composable
private fun SendPaymentScreenInternal(
    state: SendPaymentUiState,
    onInputChange: (String) -> Unit,
    onRecipientSelect: (PaymentSearchResultUiModel) -> Unit,
    onPasteClick: () -> Unit,
    onBackClick: () -> Unit,
    onScannerClick: () -> Unit,
) {
    PolkadotSurface {
        Column(
            modifier = Modifier
                .safeDrawingPadding()
                .fillMaxSize(),
        ) {
            PolkadotTopBar(
                title = stringResource(id = RCommon.string.send_payment_toolbar_title),
                navigationAction = rememberTopBarAction(
                    action = onBackClick,
                    icon = NovaIcons.Close
                ),
                actions = persistentListOf(
                    rememberTopBarAction(
                        action = onScannerClick,
                        icon = NovaIcons.Scanner
                    ),
                ),
                titleAlignment = TopBarTitleAlignment.Center,
            )

            Row(
                modifier = Modifier.padding(
                    top = 10.dp,
                    start = PolkadotTheme.spacings.mediumIncreased,
                    end = PolkadotTheme.spacings.small
                ),
                verticalAlignment = Alignment.CenterVertically
            ) {
                NovaText(
                    text = stringResource(id = RCommon.string.send_payment_to),
                    style = PolkadotTheme.typography.body.medium,
                    color = PolkadotTheme.colors.fg.secondary,
                )

                NovaTextField(
                    modifier = Modifier
                        .weight(1f)
                        .padding(horizontal = PolkadotTheme.spacings.small),
                    value = state.input,
                    maxLines = 1,
                    onValueChange = onInputChange,
                    keyboardOptions = KeyboardOptions(
                        capitalization = KeyboardCapitalization.None,
                        autoCorrectEnabled = false,
                        imeAction = ImeAction.Search
                    ),
                    placeholder = {
                        NovaText(
                            text = stringResource(id = RCommon.string.send_payment_search_placeholder),
                            style = PolkadotTheme.typography.body.large, color = PolkadotTheme.colors.fg.tertiary,
                            maxLines = 1
                        )
                    },
                )

                PolkadotTextButton(
                    size = PolkadotButtonSize.large(),
                    style = PolkadotButtonStyle.secondary(),
                    text = stringResource(RCommon.string.common_paste),
                    onClick = onPasteClick
                )
            }

            VerticalSpacer { mediumIncreased }

            when (val results = state.results) {
                is PaymentSearchResults.Sections -> SearchResult(results.sections, onRecipientSelect)
                PaymentSearchResults.Waiting -> Unit
                PaymentSearchResults.Loading -> LoadingScreenState()
                PaymentSearchResults.Empty -> if (state.input.isNotEmpty()) {
                    SendPaymentEmptySearch(state.input)
                }
            }
        }
    }
}

@Preview
@Composable
private fun SendPaymentScreenPreview() {
    PolkadotTheme {
        SendPaymentScreenInternal(
            state = SendPaymentUiState(
                input = "alice",
                results = PaymentSearchResults.Sections(
                    persistentListOf(
                        PaymentSearchSectionUiModel(
                            key = "contacts",
                            titleRes = RCommon.string.search_section_contacts,
                            items = persistentListOf(
                                PaymentSearchResultUiModel(
                                    extractedAddress = ExtractedAddress(
                                        display = "bob.dot",
                                        type = ExtractedAddress.DisplayType.USERNAME,
                                        accountId = io.paritytech.polkadotapp.common.domain.model.AccountId(ByteArray(32))
                                    ),
                                    avatarModel = AvatarUiModel.Mock.fromName("bob.dot")
                                )
                            )
                        ),
                        PaymentSearchSectionUiModel(
                            key = "general",
                            titleRes = RCommon.string.search_section_all_users,
                            items = persistentListOf(
                                PaymentSearchResultUiModel(
                                    extractedAddress = ExtractedAddress(
                                        display = "alice.dot",
                                        type = ExtractedAddress.DisplayType.USERNAME,
                                        accountId = io.paritytech.polkadotapp.common.domain.model.AccountId(ByteArray(32))
                                    ),
                                    avatarModel = AvatarUiModel.Mock.fromName("alice.dot")
                                ),
                                PaymentSearchResultUiModel(
                                    extractedAddress = ExtractedAddress(
                                        display = "34t834ug03u2093ru20fu230f9u2330r9u2r",
                                        type = ExtractedAddress.DisplayType.ADDRESS,
                                        accountId = io.paritytech.polkadotapp.common.domain.model.AccountId(ByteArray(32))
                                    ),
                                    avatarModel = AvatarUiModel.Mock.fromName("34t834ug03u2093ru20fu230f9u2330r9u2r")
                                )
                            )
                        )
                    )
                )
            ),
            onInputChange = {},
            onRecipientSelect = {},
            onPasteClick = {},
            onBackClick = {},
            onScannerClick = {},
        )
    }
}
