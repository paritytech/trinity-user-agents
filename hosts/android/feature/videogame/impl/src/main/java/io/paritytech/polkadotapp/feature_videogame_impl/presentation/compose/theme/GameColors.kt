package io.paritytech.polkadotapp.feature_videogame_impl.presentation.compose.theme

import androidx.compose.ui.graphics.Color

internal object GameColors {
    // Distinct from pure black per Figma.
    val pillChipText = Color(0xFF423925)

    // Indigo/lavender band; no LegacyNovaStableColors equivalent.
    // The same palette is used by the in-game diagonal stripe background.
    val waitingRoomBandTop = Color(0xFF292A5B)
    val waitingRoomBandHighlight = Color(0xFF5A5B95)
    val waitingRoomBandShade = Color(0xFF515286)
    val waitingRoomBandBottom = Color(0xFF2F306B)
    val waitingRoomShimmerEdge = Color(0x007B7CCF)
    val waitingRoomShimmerCore = Color(0xD99FA0E5)

    // Dark CTA background per Figma; no LegacyNovaStableColors equivalent.
    val waitingRoomCtaBackground = Color(0xFF181426)

    // Alarm red; distinct from LegacyNovaStableColors.PinkPink600.
    val countdownAlarm = Color(0xFFFF5555)

    // Player frame bezel gradients.
    val hostFrameGradientStart = Color(0xFFFFE04A)
    val hostFrameGradientMiddle = Color(0xFFFFC21B)
    val hostFrameGradientEnd = Color(0xFFE69A00)
    val hostFrameGlow = Color(0xB3FFC83C)

    val meFrameGradientStart = Color(0xFFEDF2F6)
    val meFrameGradientEnd = Color(0xFF8A95A6)
    val meFrameGlow = Color(0x99DCE2EB)

    val otherFrameGradientStart = Color(0xFF6C7BFF)
    val otherFrameGradientMiddle = Color(0xFF4A5CE8)
    val otherFrameGradientEnd = Color(0xFF2E3FB8)
    val otherFrameGlow = Color(0xB36E82FF)

    val playerFrameBackground = Color(0xFF232323)

    val tooltipBackground = Color.White
    val tooltipContent = Color.Black

    // Selection +/- overlay tint.
    val selectionPositive = Color(0xFF30D158)
    val selectionNegative = Color(0xFFFF375F)

    // Connection-state indicator; no PolkadotTheme status-green token exists.
    val connectionConnected = Color(0xFF35C759)
    val connectionConnecting = Color(0xFFFFB300)

    // "How to play" pill — intentionally identical across all app themes.
    val howToPlayBackground = Color(0xFF232323)
    val howToPlayContent = Color.White

    // Sub-round progress bar.
    val progressFill = Color(0xFF3D3E80)
    val progressTrack = Color(0xFF15152E)

    // Confirm button gradient (voting screen footer).
    val confirmGradientStart = Color(0xFF343FB7)
    val confirmGradientMiddle = Color(0xFF4A62FA)
    val confirmGradientEnd = Color(0xFF543CA5)

    // Auto-confirm countdown fill that sweeps across the confirm button.
    val confirmProgressFill = Color(0x3DFFFFFF)

    // In-game diagonal stripe background (darker variant used outside the waiting room).
    val diagonalBandGradientDark = Color(0xFF0E0E1F)

    val backgroundPrimary = Color.Black
    val textOnGameBackground = Color.White
    val lightSweepHighlight = Color.White

    val tutorialPagerInactive = Color(0xFF585B62)
    val tutorialButtonBackground = Color.White
    val tutorialButtonContent = Color.Black

    // Floating icon-button overlay tint over live video.
    val banToggleBannedBg = Color(0x14FFFFFF)
    val banToggleActiveBg = Color(0x4D000000)

    val gameTopBarBackground = Color(0x4D000000)
}
