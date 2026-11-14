package io.paritytech.polkadotapp.app.root.presentation.root.compose

import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonStyle
import io.paritytech.polkadotapp.design.components.button.icon.PolkadotIconButton
import io.paritytech.polkadotapp.design.components.button.icon.PolkadotIconButtonSize
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.Close
import io.paritytech.polkadotapp.design.components.icon.vectors.Info
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.common.R as RCommon

private val NoticeIconSize = 24.dp

@Composable
fun PaymentsSetupNotice(
    modifier: Modifier = Modifier,
    isVisible: Boolean,
    onDismissClick: () -> Unit,
) {
    val closeDescription = stringResource(RCommon.string.common_close)

    TopNotice(modifier = modifier, isVisible = isVisible) {
        NovaIcon(
            modifier = Modifier.size(NoticeIconSize),
            imageVector = NovaIcons.Info,
            tint = PolkadotTheme.colors.fg.secondary,
        )

        NovaText(
            modifier = Modifier.weight(1f),
            text = stringResource(RCommon.string.payments_setup_notice_message),
            style = PolkadotTheme.typography.body.medium,
            color = PolkadotTheme.colors.fg.primary,
        )

        PolkadotIconButton(
            modifier = Modifier.semantics { contentDescription = closeDescription },
            icon = NovaIcons.Close,
            onClick = onDismissClick,
            style = PolkadotButtonStyle.ghost(),
            size = PolkadotIconButtonSize.small(),
            shape = PolkadotTheme.shapes.full,
        )
    }
}

@Preview
@Composable
private fun PaymentsSetupNoticePreview() {
    PaymentsSetupNotice(
        modifier = Modifier.fillMaxSize(),
        isVisible = true,
        onDismissClick = {},
    )
}
