package io.paritytech.polkadotapp.feature_sso_impl.presentation.pairRequest.compose.components

import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.ColorFilter
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonShape
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonStyle
import io.paritytech.polkadotapp.design.components.button.default.PolkadotTextButton
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_sso_api.presentation.formatHostVersionLabel
import io.paritytech.polkadotapp.feature_sso_impl.presentation.pairRequest.PairRequestDeviceUiModel
import io.paritytech.polkadotapp.common.R as RCommon
import io.paritytech.polkadotapp.design.R as RDesign

@Composable
internal fun PairRequestDialogColumn(
    verticalArrangement: Arrangement.Vertical,
    content: @Composable ColumnScope.() -> Unit,
) {
    Column(
        modifier = Modifier
            .fillMaxWidth()
            .padding(
                start = PolkadotTheme.spacings.mediumIncreased,
                end = PolkadotTheme.spacings.mediumIncreased,
                top = PolkadotTheme.spacings.extraLargeIncreased,
                bottom = PolkadotTheme.spacings.large,
            ),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = verticalArrangement,
        content = content
    )
}

@Composable
internal fun PairRequestDialogTitle(text: String) {
    NovaText(
        modifier = Modifier.fillMaxWidth(),
        text = text,
        style = PolkadotTheme.typography.headline.small,
        color = PolkadotTheme.colors.fg.primary,
        textAlign = TextAlign.Center
    )
}

@Composable
internal fun PairRequestInfoCard(content: @Composable ColumnScope.() -> Unit) {
    PolkadotSurface(
        modifier = Modifier.fillMaxWidth(),
        color = PolkadotTheme.colors.bg.surface.nested,
        shape = PolkadotTheme.shapes.mediumIncreased
    ) {
        Column(
            modifier = Modifier.padding(PolkadotTheme.spacings.large),
            verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.small),
            content = content
        )
    }
}

@Composable
internal fun PairRequestTwoActionRow(
    cancelEnabled: Boolean,
    primaryEnabled: Boolean,
    primaryLoading: Boolean,
    primaryText: String,
    onCancelClicked: () -> Unit,
    onPrimaryClicked: () -> Unit,
) {
    Column(
        modifier = Modifier.fillMaxWidth(),
        verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.mediumIncreased)
    ) {
        PolkadotTextButton(
            modifier = Modifier.fillMaxWidth(),
            text = primaryText,
            style = PolkadotButtonStyle.primary(),
            shape = PolkadotButtonShape.pill,
            enabled = primaryEnabled,
            loading = primaryLoading,
            onClick = onPrimaryClicked
        )

        PolkadotTextButton(
            modifier = Modifier.fillMaxWidth(),
            text = stringResource(RCommon.string.pair_request_cancel_action),
            style = PolkadotButtonStyle.ghost(),
            shape = PolkadotButtonShape.pill,
            enabled = cancelEnabled,
            onClick = onCancelClicked
        )
    }
}

@Composable
internal fun PairRequestDeviceHeader(device: PairRequestDeviceUiModel) {
    Column(
        modifier = Modifier.fillMaxWidth(),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.mediumIncreased)
    ) {
        Image(
            modifier = Modifier.size(width = 80.dp, height = 68.dp),
            painter = painterResource(RDesign.drawable.img_link_device_pixel_computer),
            contentScale = ContentScale.Inside,
            colorFilter = ColorFilter.tint(PolkadotTheme.colors.fg.primary),
            contentDescription = null
        )

        DeviceInfoText(device = device)
    }
}

@Composable
private fun DeviceInfoText(device: PairRequestDeviceUiModel) {
    val title = if (device.hostVersion != null) {
        "${device.name} ${formatHostVersionLabel(device.hostVersion)}"
    } else {
        device.name
    }
    val platformLabel = device.platformType?.let { stringResource(RCommon.string.pair_request_device_platform, it) }
    val combined = if (platformLabel != null) "$title\n$platformLabel" else title

    NovaText(
        modifier = Modifier.fillMaxWidth(),
        text = combined,
        color = PolkadotTheme.colors.fg.primary,
        style = PolkadotTheme.typography.body.large,
        textAlign = TextAlign.Center
    )
}
