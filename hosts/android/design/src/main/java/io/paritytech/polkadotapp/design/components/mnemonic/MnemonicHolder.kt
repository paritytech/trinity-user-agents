package io.paritytech.polkadotapp.design.components.mnemonic

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.itemsIndexed
import androidx.compose.foundation.text.TextAutoSize
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.layout.Layout
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.withStyle
import io.paritytech.polkadotapp.design.components.mnemonic.model.Word
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import kotlinx.collections.immutable.ImmutableList

private const val ColumnsCount = 3

@Composable
internal fun MnemonicHolder(
    modifier: Modifier = Modifier,
    mnemonic: ImmutableList<Word>,
    onWordClickAction: ((Word) -> Unit)? = null
) {
    PolkadotSurface(
        modifier = modifier,
        color = PolkadotTheme.colors.bg.surface.container,
        shape = PolkadotTheme.shapes.medium
    ) {
        val spacings = PolkadotTheme.spacings

        Layout(
            modifier = Modifier.padding(spacings.extraMedium),
            content = {
                MnemonicWord(word = "template")

                LazyVerticalGrid(
                    modifier = Modifier.fillMaxWidth(),
                    columns = GridCells.Fixed(ColumnsCount),
                    verticalArrangement = Arrangement.spacedBy(spacings.tiny),
                    horizontalArrangement = Arrangement.spacedBy(spacings.tiny, Alignment.CenterHorizontally)
                ) {
                    itemsIndexed(
                        items = mnemonic,
                        key = { index, word -> "$index-$word" }
                    ) { index, word ->
                        MnemonicWord(
                            modifier = Modifier.animateItem(),
                            index = index + 1,
                            word = word.value,
                            onClick = if (onWordClickAction != null) {
                                { onWordClickAction(word) }
                            } else null
                        )
                    }
                }
            }
        ) { measurables, constraints ->
            val (wordTemplateMeasurable, gridMeasurable) = measurables
            val itemMargin = spacings.tiny.roundToPx()

            val gridHeight = wordTemplateMeasurable.measure(constraints).height * 4 + itemMargin * 3
            val gridPlaceable = gridMeasurable.measure(
                constraints.copy(
                    minHeight = gridHeight,
                    maxHeight = gridHeight
                )
            )

            layout(constraints.maxWidth, gridHeight) {
                gridPlaceable.placeRelative(0, 0)
            }
        }
    }
}

@Composable
fun MnemonicWord(
    modifier: Modifier = Modifier,
    index: Int? = null,
    word: String,
    onClick: (() -> Unit)? = null,
    enabled: Boolean = true
) {
    PolkadotSurface(
        modifier = modifier,
        shape = PolkadotTheme.shapes.large,
        color = PolkadotTheme.colors.bg.surface.nested,
        onClick = onClick,
        enabled = enabled
    ) {
        NovaText(
            modifier = Modifier.padding(PolkadotTheme.spacings.small),
            text = buildAnnotatedString {
                if (index != null) {
                    withStyle(
                        style = SpanStyle(color = PolkadotTheme.colors.fg.tertiary)
                    ) {
                        append(index.toString())
                    }
                    append(" ")
                }
                append(word)
            },
            style = PolkadotTheme.typography.body.large,
            color = PolkadotTheme.colors.fg.primary,
            textAlign = if (index != null) TextAlign.Start else TextAlign.Center,
            maxLines = 1,
            autoSize = TextAutoSize.StepBased(
                minFontSize = PolkadotTheme.typography.body.large.fontSize / 2,
                maxFontSize = PolkadotTheme.typography.body.large.fontSize,
            )
        )
    }
}
