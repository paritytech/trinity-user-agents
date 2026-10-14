package io.paritytech.polkadotapp.feature_settings_impl.presentation.theme.compose.components

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.scaleIn
import androidx.compose.animation.scaleOut
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.wrapContentWidth
import androidx.compose.foundation.rememberScrollState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.dropShadow
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.shadow.Shadow
import androidx.compose.ui.layout.boundsInRoot
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.unit.DpOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.util.fastForEach
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.designsystem.colors.PolkadotColorsPalette
import io.paritytech.polkadotapp.designsystem.themes.PolkadotAppTheme
import kotlinx.collections.immutable.ImmutableList

@Composable
internal fun ThemePicker(
    themes: ImmutableList<PolkadotAppTheme>,
    selectedTheme: PolkadotAppTheme,
    onThemeSelected: (PolkadotAppTheme, Offset) -> Unit
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .horizontalScroll(rememberScrollState())
            .padding(horizontal = PolkadotTheme.spacings.large),
        horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.large)
    ) {
        themes.fastForEach {
            Theme(
                theme = it,
                isSelected = it == selectedTheme,
                onThemeSelected = onThemeSelected
            )
        }
    }
}

@Composable
private fun Theme(
    theme: PolkadotAppTheme,
    isSelected: Boolean,
    onThemeSelected: (PolkadotAppTheme, Offset) -> Unit
) {
    val palette = remember(theme) { theme.colors() }
    var dotCenterInRoot by remember { mutableStateOf(Offset.Zero) }

    Column(
        modifier = Modifier
            .width(48.dp)
            .clickable(onClick = { onThemeSelected(theme, dotCenterInRoot) }),
        horizontalAlignment = Alignment.CenterHorizontally
    ) {
        Dot(
            palette = palette,
            isSelected = isSelected,
            onCenterChanged = { dotCenterInRoot = it }
        )

        VerticalSpacer { small }

        ThemeName(theme.id, isSelected)
    }
}

@Composable
private fun Dot(
    palette: PolkadotColorsPalette,
    isSelected: Boolean,
    onCenterChanged: (Offset) -> Unit
) {
    Box(
        modifier = Modifier
            .size(48.dp)
            .onGloballyPositioned { onCenterChanged(it.boundsInRoot().center) }
            .dropShadow(
                shape = PolkadotTheme.shapes.full,
                shadow = Shadow(
                    radius = 4.dp,
                    color = Color.Black,
                    offset = DpOffset(0.dp, 2.dp),
                    alpha = 0.25f
                )
            )
            .clip(PolkadotTheme.shapes.full)
            .background(palette.bg.action.secondary),
        contentAlignment = Alignment.Center
    ) {
        AnimatedVisibility(
            visible = isSelected,
            enter = scaleIn() + fadeIn(),
            exit = scaleOut() + fadeOut()
        ) {
            Box(
                Modifier
                    .size(24.dp)
                    .clip(PolkadotTheme.shapes.full)
                    .background(palette.bg.surface.containerInverted)
            )
        }
    }
}

@Composable
private fun ThemeName(
    name: String,
    isSelected: Boolean
) {
    AnimatedVisibility(
        modifier = Modifier.wrapContentWidth(unbounded = true),
        visible = isSelected,
        enter = scaleIn() + fadeIn(),
        exit = scaleOut() + fadeOut()
    ) {
        NovaText(
            text = name,
            style = PolkadotTheme.typography.title.small,
            maxLines = 1,
            softWrap = false
        )
    }
}
