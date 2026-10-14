package io.paritytech.polkadotapp.feature_sso_impl.presentation.pairRequest.compose.components

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonShape
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonStyle
import io.paritytech.polkadotapp.design.components.button.default.PolkadotTextButton
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
internal fun LimitReachedContent(
    totalSlots: Int,
    onCloseClicked: () -> Unit,
) {
    PairRequestDialogColumn(verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.extraLarge)) {
        PairRequestDialogTitle(stringResource(RCommon.string.pair_request_limit_reached_title))

        NovaText(
            modifier = Modifier.fillMaxWidth(),
            text = stringResource(RCommon.string.pair_request_limit_reached_message, totalSlots),
            color = PolkadotTheme.colors.fg.primary,
            style = PolkadotTheme.typography.body.large,
            textAlign = TextAlign.Center
        )

        PolkadotTextButton(
            modifier = Modifier.fillMaxWidth(),
            text = stringResource(RCommon.string.pair_request_close_action),
            shape = PolkadotButtonShape.pill,
            style = PolkadotButtonStyle.ghost(),
            onClick = onCloseClicked
        )
    }
}
