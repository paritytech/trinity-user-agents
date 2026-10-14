package io.paritytech.polkadotapp.design.components.mnemonic

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.design.components.mnemonic.model.Word
import io.paritytech.polkadotapp.design.components.mnemonic.model.toWordList
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import kotlinx.collections.immutable.ImmutableList
import kotlinx.collections.immutable.toImmutableList

@Composable
fun ProtectedMnemonic(
    mnemonic: ImmutableList<Word>,
    onRevealMnemonicAction: () -> Unit,
    isHidden: Boolean,
    coverTitle: String,
    coverDescription: String,
    onWordClickAction: ((Word) -> Unit)? = null
) {
    Box(
        modifier = Modifier.padding(horizontal = PolkadotTheme.spacings.mediumIncreased)
    ) {
        MnemonicHolder(
            mnemonic = mnemonic,
            onWordClickAction = onWordClickAction
        )

        val coverAlpha by animateFloatAsState(targetValue = if (isHidden) 1f else 0f)
        if (coverAlpha > 0) {
            Cover(
                modifier = Modifier
                    .matchParentSize()
                    .clip(PolkadotTheme.shapes.medium)
                    .clickable(onClick = onRevealMnemonicAction)
                    .alpha(coverAlpha),
                title = coverTitle,
                description = coverDescription
            )
        }
    }
}

@Composable
fun Mnemonic(
    mnemonic: ImmutableList<Word>,
    onWordClickAction: ((Word) -> Unit)? = null
) {
    Box(
        modifier = Modifier.padding(horizontal = PolkadotTheme.spacings.mediumIncreased)
    ) {
        MnemonicHolder(
            mnemonic = mnemonic,
            onWordClickAction = onWordClickAction
        )
    }
}

@Composable
private fun Cover(
    modifier: Modifier,
    title: String,
    description: String
) {
    Box(modifier = modifier) {
        BlurredFakeMnemonic(
            modifier = Modifier.fillMaxSize(),
            surfaceColor = PolkadotTheme.colors.bg.surface.container,
            wordColor = PolkadotTheme.colors.bg.surface.nested
        )

        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(56.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.Center
        ) {
            NovaText(
                text = title,
                style = PolkadotTheme.typography.title.medium,
                color = PolkadotTheme.colors.fg.primary
            )

            VerticalSpacer { small }

            NovaText(
                text = description,
                style = PolkadotTheme.typography.body.medium,
                color = PolkadotTheme.colors.fg.tertiary,
                textAlign = TextAlign.Center
            )
        }
    }
}

private val previewMnemonic = listOf(
    "scout", "ribbon", "velvet", "harbor", "puzzle", "anchor",
    "meadow", "signal", "orbit", "cradle", "thunder", "wisdom"
).toWordList().toImmutableList()

@Preview(backgroundColor = 0xFF191919, showBackground = true)
@Composable
private fun MnemonicPreview() {
    PolkadotTheme {
        Mnemonic(mnemonic = previewMnemonic)
    }
}

@Preview(backgroundColor = 0xFF191919, showBackground = true)
@Composable
private fun ProtectedMnemonicPreview() {
    PolkadotTheme {
        ProtectedMnemonic(
            mnemonic = previewMnemonic,
            onRevealMnemonicAction = {},
            isHidden = true,
            coverTitle = "Tap to reveal",
            coverDescription = "Make sure no one is watching your screen"
        )
    }
}
