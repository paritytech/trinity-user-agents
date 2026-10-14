package io.paritytech.polkadotapp.feature_settings_impl.presentation.backup.status.compose.components

import androidx.compose.foundation.layout.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme

@Composable
fun BackupStatusStateContent(
    image: @Composable () -> Unit,
    header: String,
    description: String?,
    footerContent: @Composable ColumnScope.() -> Unit
) {
    Column(
        modifier = Modifier.fillMaxSize()
    ) {
        Spacer(Modifier.weight(0.15f))

        Box(
            modifier = Modifier
                .fillMaxWidth()
                .weight(0.55f),
        ) {
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(horizontal = 48.dp),
                horizontalAlignment = Alignment.CenterHorizontally
            ) {
                ImageContainer(image)

                VerticalSpacer { large }

                NovaText(
                    text = header,
                    style = PolkadotTheme.typography.headline.small,
                    color = PolkadotTheme.colors.fg.primary,
                    textAlign = TextAlign.Center
                )

                VerticalSpacer { small }

                description?.let {
                    NovaText(
                        text = it,
                        style = PolkadotTheme.typography.body.large,
                        color = PolkadotTheme.colors.fg.secondary,
                        textAlign = TextAlign.Center
                    )
                }
            }
        }

        Box(
            modifier = Modifier
                .fillMaxWidth()
                .padding(PolkadotTheme.spacings.large)
                .weight(0.3f),
            contentAlignment = Alignment.BottomCenter
        ) {
            Column(
                verticalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.small),
                content = footerContent
            )
        }
    }
}

@Composable
private fun ImageContainer(
    content: @Composable () -> Unit
) {
    PolkadotSurface(
        modifier = Modifier.size(104.dp),
        shape = PolkadotTheme.shapes.full,
        color = Color(0x1FFFFFFF)
    ) {
        Box(
            contentAlignment = Alignment.Center
        ) {
            content()
        }
    }
}
