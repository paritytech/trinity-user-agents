package io.paritytech.polkadotapp.feature_products_impl.presentation.signTransaction.compose.loaded

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonStyle
import io.paritytech.polkadotapp.design.components.button.default.PolkadotTextButton
import io.paritytech.polkadotapp.design.components.image.NovaAsyncImage
import io.paritytech.polkadotapp.design.components.spacer.HorizontalSpacer
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_products_impl.presentation.signTransaction.SigningAccountUi
import io.paritytech.polkadotapp.feature_products_impl.presentation.signTransaction.SigningContent
import io.paritytech.polkadotapp.feature_products_impl.presentation.signTransaction.TransactionSignUiState
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
fun MainContent(
    state: TransactionSignUiState,
    onApproveClicked: () -> Unit,
    onRejectClicked: () -> Unit,
    onDetailsClicked: () -> Unit,
) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .verticalScroll(rememberScrollState())
            .padding(PolkadotTheme.spacings.mediumIncreased),
        horizontalAlignment = Alignment.CenterHorizontally
    ) {
        VerticalSpacer { mediumIncreased }

        NovaAsyncImage(
            modifier = Modifier
                .size(64.dp)
                .clip(PolkadotTheme.shapes.full),
            model = state.requesterIconUrl,
            contentDescription = state.requesterName,
            contentScale = ContentScale.Crop
        )

        VerticalSpacer { mediumIncreased }

        NovaText(
            modifier = Modifier.padding(horizontal = PolkadotTheme.spacings.extraLarge),
            textAlign = TextAlign.Center,
            text = stringResource(RCommon.string.sign_transaction_title, state.requesterName),
            style = PolkadotTheme.typography.headline.small,
            color = PolkadotTheme.colors.fg.primary,
        )

        VerticalSpacer { small }

        NovaText(
            modifier = Modifier.padding(horizontal = PolkadotTheme.spacings.extraLarge),
            textAlign = TextAlign.Center,
            text = when (state.content) {
                is SigningContent.Transaction -> state.content.callName
                is SigningContent.RawMessage -> state.content.hexData
                is SigningContent.VrfTranscript -> state.content.transcriptLabel
            },
            style = PolkadotTheme.typography.body.large,
            color = PolkadotTheme.colors.fg.tertiary,
            maxLines = 4,
            overflow = TextOverflow.Ellipsis
        )

        VerticalSpacer { mediumIncreased }

        SigningAccountSection(state.signingAccount)

        VerticalSpacer { mediumIncreased }

        PolkadotTextButton(
            modifier = Modifier.fillMaxWidth(),
            text = stringResource(RCommon.string.sign_transaction_view_details),
            style = PolkadotButtonStyle.secondary(),
            onClick = onDetailsClicked
        )

        VerticalSpacer { mediumIncreased }

        Row {
            PolkadotTextButton(
                modifier = Modifier.weight(1f),
                text = stringResource(RCommon.string.sign_transaction_reject),
                style = PolkadotButtonStyle.secondary(),
                enabled = !state.signing,
                onClick = onRejectClicked
            )

            HorizontalSpacer { small }

            PolkadotTextButton(
                modifier = Modifier.weight(1f),
                text = stringResource(RCommon.string.sign_transaction_approve),
                style = PolkadotButtonStyle.primary(),
                loading = state.signing,
                onClick = onApproveClicked
            )
        }
    }
}

@Composable
private fun SigningAccountSection(signingAccount: SigningAccountUi) {
    Row(
        modifier = Modifier.fillMaxWidth(),
        verticalAlignment = Alignment.CenterVertically
    ) {
        NovaText(
            text = stringResource(RCommon.string.sign_transaction_signing_account),
            style = PolkadotTheme.typography.body.large,
            color = PolkadotTheme.colors.fg.tertiary,
        )

        HorizontalSpacer { small }

        NovaText(
            text = when (signingAccount) {
                is SigningAccountUi.Product -> stringResource(
                    RCommon.string.sign_transaction_product_account,
                    signingAccount.productId,
                    signingAccount.derivationIndex
                )

                SigningAccountUi.IdentityAccount -> stringResource(RCommon.string.sign_transaction_identity_account)
            },
            style = PolkadotTheme.typography.body.large,
            color = PolkadotTheme.colors.fg.primary,
        )
    }
}
