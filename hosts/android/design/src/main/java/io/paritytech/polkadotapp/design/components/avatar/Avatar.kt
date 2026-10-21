package io.paritytech.polkadotapp.design.components.avatar

import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.*
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.painter.Painter
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.text.TextMeasurer
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.drawText
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.toOffset
import io.paritytech.polkadotapp.design.components.image.NovaAsyncImage
import io.paritytech.polkadotapp.design.components.spacer.HorizontalSpacer
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.configs.colors.AvatarColorScheme
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.designsystem.typography.PolkadotTypography
import kotlin.math.abs
import kotlin.math.roundToInt

sealed class AvatarUiModel {
    companion object;

    /** [placeholder] shows until the image at [url] has loaded, and stays if it never does. */
    data class Image(val url: String, val placeholder: Name? = null) : AvatarUiModel()

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
            placeholder = model.placeholder,
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
    placeholder: AvatarUiModel.Name?,
    onClick: (() -> Unit)? = null
) {
    PolkadotSurface(
        modifier = modifier,
        shape = PolkadotTheme.shapes.full,
        color = PolkadotTheme.colors.bg.surface.container,
        onClick = onClick
    ) {
        val letter = placeholder?.let { rememberLetterAvatarPainter(it.name, it.colorScheme) }

        NovaAsyncImage(
            model = url,
            contentScale = ContentScale.Crop,
            placeholder = letter,
            error = letter
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
    PolkadotSurface(
        modifier = modifier,
        shape = PolkadotTheme.shapes.full,
        color = colorScheme.background,
        onClick = onClick
    ) {
        Image(
            painter = rememberLetterAvatarPainter(name, colorScheme),
            contentDescription = null,
            modifier = Modifier.fillMaxSize()
        )
    }
}

/** The letter avatar as a [Painter], so it can also stand in for an image that is loading or failed to load. */
@Composable
private fun rememberLetterAvatarPainter(name: String, colorScheme: AvatarColorScheme): Painter {
    val background = colorScheme.background
    val foreground = colorScheme.foreground
    val typography = PolkadotTheme.typography
    val textMeasurer = rememberTextMeasurer()

    return remember(name, background, foreground, typography, textMeasurer) {
        LetterAvatarPainter(
            letter = name.firstOrNull()?.uppercase() ?: "?",
            background = background,
            foreground = foreground,
            typography = typography,
            textMeasurer = textMeasurer
        )
    }
}

private class LetterAvatarPainter(
    private val letter: String,
    private val background: Color,
    private val foreground: Color,
    private val typography: PolkadotTypography,
    private val textMeasurer: TextMeasurer,
) : Painter() {
    override val intrinsicSize: Size = Size.Unspecified

    override fun DrawScope.onDraw() {
        drawRect(background)

        val style = avatarTextStyle(typography, size.minDimension.toDp()).copy(color = foreground, textAlign = TextAlign.Center)
        val layout = textMeasurer.measure(letter, style)
        // Centred the way a Box centres its content, rounded to whole pixels.
        val topLeft = Alignment.Center.align(layout.size, IntSize(size.width.roundToInt(), size.height.roundToInt()), layoutDirection)
        drawText(textLayoutResult = layout, topLeft = topLeft.toOffset())
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

private fun avatarTextStyle(typography: PolkadotTypography, boxSize: Dp): TextStyle {
    val nearest = AVATAR_TEXT_STYLES.keys.minBy { abs((it - boxSize).value) }

    return AVATAR_TEXT_STYLES.getValue(nearest)(typography)
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
