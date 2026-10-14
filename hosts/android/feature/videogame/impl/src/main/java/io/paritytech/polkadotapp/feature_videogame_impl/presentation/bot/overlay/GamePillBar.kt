package io.paritytech.polkadotapp.feature_videogame_impl.presentation.bot.overlay

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.common.presentation.formatters.time.LocalTimeFormatter
import io.paritytech.polkadotapp.common.presentation.formatters.time.TimeFormatter
import io.paritytech.polkadotapp.design.colors.LegacyNovaStableColors
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.ArrowDropdown
import io.paritytech.polkadotapp.design.components.spacer.HorizontalSpacer
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_videogame_impl.presentation.compose.theme.GameColors
import io.paritytech.polkadotapp.feature_videogame_impl.presentation.compose.theme.NovaGameTypography
import kotlin.time.Duration.Companion.seconds
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
internal fun GamePillBar(
    modifier: Modifier = Modifier,
    state: VideoGamePillState.Shown,
    showChevron: Boolean,
    onClick: (() -> Unit)?,
) {
    PolkadotSurface(
        modifier = modifier,
        shape = PolkadotTheme.shapes.full,
        color = LegacyNovaStableColors.NeutralNeutral800,
        contentColor = LegacyNovaStableColors.AmberAmber500,
        border = BorderStroke(PILL_BORDER_WIDTH, LegacyNovaStableColors.AmberAmber500),
        onClick = onClick,
    ) {
        Row(
            modifier = Modifier.padding(
                start = PolkadotTheme.spacings.mediumIncreased,
                end = PolkadotTheme.spacings.small,
                top = PolkadotTheme.spacings.small,
                bottom = PolkadotTheme.spacings.small,
            ),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            NovaText(
                text = stringResource(pillLabelRes(state)).uppercase(),
                style = NovaGameTypography.pillText,
                color = LegacyNovaStableColors.AmberAmber500,
            )

            HorizontalSpacer { small }

            Row(verticalAlignment = Alignment.CenterVertically) {
                pillChipText(state)?.let { chipText ->
                    ChipCapsule(text = chipText)
                }

                if (showChevron) {
                    HorizontalSpacer { small }
                    ChevronUpBadge()
                }
            }
        }
    }
}

@Composable
private fun ChipCapsule(text: String) {
    PolkadotSurface(
        shape = PolkadotTheme.shapes.full,
        color = LegacyNovaStableColors.AmberAmber500,
        contentColor = GameColors.pillChipText,
    ) {
        NovaText(
            modifier = Modifier.padding(
                horizontal = PolkadotTheme.spacings.extraMedium,
                vertical = PolkadotTheme.spacings.tiny,
            ),
            text = text,
            style = NovaGameTypography.pillText,
            color = GameColors.pillChipText,
        )
    }
}

@Composable
private fun ChevronUpBadge() {
    PolkadotSurface(
        modifier = Modifier.size(CHEVRON_BADGE_SIZE),
        shape = PolkadotTheme.shapes.full,
        color = LegacyNovaStableColors.AmberAmber500,
        contentColor = Color.Black,
        contentAlignment = Alignment.Center,
    ) {
        NovaIcon(
            modifier = Modifier
                .size(CHEVRON_ICON_SIZE)
                .rotate(CHEVRON_UP_ROTATION),
            imageVector = NovaIcons.ArrowDropdown,
            tint = Color.Black,
        )
    }
}

private fun pillLabelRes(state: VideoGamePillState.Shown): Int = when (state) {
    is VideoGamePillState.Shown.WaitingCountdown -> RCommon.string.video_game_pill_waiting_countdown_label
    is VideoGamePillState.Shown.InProgress -> RCommon.string.video_game_pill_in_progress_label
    VideoGamePillState.Shown.Review -> RCommon.string.video_game_pill_review_label
}

@Composable
private fun pillChipText(state: VideoGamePillState.Shown): String? = when (state) {
    is VideoGamePillState.Shown.WaitingCountdown ->
        LocalTimeFormatter.current.formatCountdown(state.secondsLeft.seconds)
    is VideoGamePillState.Shown.InProgress -> stringResource(
        RCommon.string.video_game_pill_round_template,
        state.currentRound,
        state.totalRounds,
    )
    VideoGamePillState.Shown.Review -> null
}

private val PILL_BORDER_WIDTH = 3.dp
private val CHEVRON_BADGE_SIZE = 28.dp
private val CHEVRON_ICON_SIZE = 20.dp
private const val CHEVRON_UP_ROTATION = 180f

// region Previews

@Preview
@Composable
private fun GamePillWaitingPreview() {
    PolkadotTheme {
        CompositionLocalProvider(
            LocalTimeFormatter provides TimeFormatter.mocked(LocalContext.current)
        ) {
            GamePillBar(
                modifier = Modifier.padding(PolkadotTheme.spacings.mediumIncreased),
                state = VideoGamePillState.Shown.WaitingCountdown(secondsLeft = 87),
                showChevron = true,
                onClick = {},
            )
        }
    }
}

@Preview
@Composable
private fun GamePillInProgressPreview() {
    PolkadotTheme {
        GamePillBar(
            modifier = Modifier.padding(PolkadotTheme.spacings.mediumIncreased),
            state = VideoGamePillState.Shown.InProgress(currentRound = 3, totalRounds = 12),
            showChevron = true,
            onClick = {},
        )
    }
}

@Preview
@Composable
private fun GamePillOnGameScreenPreview() {
    PolkadotTheme {
        CompositionLocalProvider(
            LocalTimeFormatter provides TimeFormatter.mocked(LocalContext.current)
        ) {
            GamePillBar(
                modifier = Modifier.padding(PolkadotTheme.spacings.mediumIncreased),
                state = VideoGamePillState.Shown.WaitingCountdown(secondsLeft = 87),
                showChevron = false,
                onClick = null,
            )
        }
    }
}
