package io.paritytech.polkadotapp.design.components.avatar

import androidx.compose.foundation.layout.*
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.image.NovaAsyncImage
import io.paritytech.polkadotapp.design.components.spacer.HorizontalSpacer
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.configs.colors.AvatarColorScheme
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.designsystem.typography.PolkadotTypography
import kotlin.math.abs

sealed class AvatarUiModel {
    companion object;

    data class Image(val url: String) : AvatarUiModel()

    data class Name(val name: String, val colorScheme: AvatarColorScheme) : AvatarUiModel()
}

@Composable
fun PolkadotAvatar(
    model: AvatarUiModel,
    modifier: Modifier,
    onClick: (() -> Unit)? = null
) {
    when (model) {
        is AvatarUiModel.Image -> ImageAvatar(
            modifier = modifier,
            url = model.url,
            onClick = onClick
        )

        is AvatarUiModel.Name -> LetterAvatar(
            modifier = modifier,
            name = model.name,
            colorScheme = model.colorScheme,
            onClick = onClick
        )
    }
}

@Composable
private fun ImageAvatar(
    modifier: Modifier,
    url: String,
    onClick: (() -> Unit)? = null
) {
    PolkadotSurface(
        modifier = modifier,
        shape = PolkadotTheme.shapes.full,
        color = PolkadotTheme.colors.bg.surface.container,
        onClick = onClick
    ) {
        NovaAsyncImage(
            model = url,
            contentScale = ContentScale.Crop
        )
    }
}

@Composable
private fun LetterAvatar(
    modifier: Modifier,
    name: String,
    colorScheme: AvatarColorScheme,
    onClick: (() -> Unit)?
) {
    val firstLetter = name.firstOrNull()?.uppercase() ?: "?"

    PolkadotSurface(
        modifier = modifier,
        shape = PolkadotTheme.shapes.full,
        color = colorScheme.background,
        onClick = onClick
    ) {
        BoxWithConstraints(
            contentAlignment = Alignment.Center
        ) {
            val boxSize = if (maxWidth < maxHeight) maxWidth else maxHeight

            NovaText(
                text = firstLetter,
                style = rememberAvatarTextStyle(boxSize),
                color = colorScheme.foreground,
                textAlign = TextAlign.Center
            )
        }
    }
}

private val AVATAR_TEXT_STYLES: Map<Dp, (PolkadotTypography) -> TextStyle> = mapOf(
    20.dp to { it.title.tinyEmphasized },
    28.dp to { it.title.extraLarge },
    40.dp to { it.headline.small },
    44.dp to { it.headline.medium },
    48.dp to { it.headline.medium },
    56.dp to { it.headline.large },
    64.dp to { it.headline.large },
    72.dp to { it.display.small },
    108.dp to { it.display.large },
    136.dp to { it.display.extraLarge }
)

@Composable
private fun rememberAvatarTextStyle(boxSize: Dp): TextStyle {
    val typography = PolkadotTheme.typography

    val style = remember(boxSize) {
        val nearest = AVATAR_TEXT_STYLES.keys.minBy { abs((it - boxSize).value) }
        AVATAR_TEXT_STYLES.getValue(nearest)(typography)
    }

    return style
}

object AvatarModelMocks {
    fun fromName(name: String) = AvatarUiModel.Name(
        name = name,
        colorScheme = AvatarColorScheme.from(name.encodeToByteArray())
    )
}

val AvatarUiModel.Companion.Mock: AvatarModelMocks get() = AvatarModelMocks

@Preview(backgroundColor = 0xff000000, showBackground = true)
@Composable
private fun AvatarColorSchemesPreview() {
    PolkadotTheme {
        Column {
            AvatarColorScheme.entries.forEach { scheme ->
                Row(verticalAlignment = Alignment.CenterVertically) {
                    PolkadotAvatar(
                        model = AvatarUiModel.Name(name = scheme.name, colorScheme = scheme),
                        modifier = Modifier.size(48.dp)
                    )
                    HorizontalSpacer { small }
                    NovaText(text = scheme.name)
                }
                VerticalSpacer { small }
            }
        }
    }
}
