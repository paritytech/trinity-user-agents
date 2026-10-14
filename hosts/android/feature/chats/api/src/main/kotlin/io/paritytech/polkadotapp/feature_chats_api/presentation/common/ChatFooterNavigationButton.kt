package io.paritytech.polkadotapp.feature_chats_api.presentation.common

import androidx.compose.foundation.layout.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.button.common.PolkadotButtonStyle
import io.paritytech.polkadotapp.design.components.button.default.PolkadotButtonSize
import io.paritytech.polkadotapp.design.components.button.default.PolkadotTextButton
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme

@Composable
fun ChatFooterNavigationButton(
    modifier: Modifier = Modifier,
    title: String,
    description: String,
    actionName: String,
    onClick: () -> Unit
) {
    PolkadotSurface(
        modifier = modifier,
        shape = PolkadotTheme.shapes.mediumIncreased,
        color = PolkadotTheme.colors.bg.surface.nested,
    ) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(PolkadotTheme.spacings.mediumIncreased),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.mediumIncreased)
        ) {
            PolkadotSurface(
                modifier = Modifier.size(48.dp),
                shape = PolkadotTheme.shapes.full,
                color = Color(0x1FFFFFFF)
            ) {}

            Column(Modifier.weight(1f)) {
                NovaText(
                    text = title,
                    style = PolkadotTheme.typography.body.mediumEmphasized,
                    color = PolkadotTheme.colors.fg.primary,
                    maxLines = 1
                )

                VerticalSpacer { tiny }

                NovaText(
                    text = description,
                    style = PolkadotTheme.typography.body.medium,
                    color = PolkadotTheme.colors.fg.tertiary,
                    maxLines = 2
                )
            }

            PolkadotTextButton(
                text = actionName,
                size = PolkadotButtonSize.medium(),
                style = PolkadotButtonStyle.secondary(),
                onClick = onClick
            )
        }
    }
}

@Preview(backgroundColor = 0x000000)
@Composable
private fun ChatFooterNavigationButtonPreview() {
    PolkadotTheme {
        ChatFooterNavigationButton(
            title = "title",
            description = "description",
            actionName = "Open",
            onClick = {}
        )
    }
}
