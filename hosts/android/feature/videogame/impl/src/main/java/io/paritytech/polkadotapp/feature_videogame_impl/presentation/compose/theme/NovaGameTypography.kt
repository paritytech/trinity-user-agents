package io.paritytech.polkadotapp.feature_videogame_impl.presentation.compose.theme

import androidx.compose.ui.text.ExperimentalTextApi
import androidx.compose.ui.text.PlatformTextStyle
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontVariation
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.LineHeightStyle
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.sp
import io.paritytech.polkadotapp.designsystem.typography.PolkadotFontFamilies
import io.paritytech.polkadotapp.feature_videogame_impl.R

@OptIn(ExperimentalTextApi::class)
private fun mulishVariable(weight: FontWeight) = Font(
    resId = R.font.mulish_variable,
    weight = weight,
    variationSettings = FontVariation.Settings(FontVariation.weight(weight.weight))
)

private val ExtraBlack = FontWeight(1000)

private val mulish = FontFamily(
    mulishVariable(FontWeight.Normal),
    mulishVariable(FontWeight.Medium),
    mulishVariable(FontWeight.SemiBold),
    mulishVariable(FontWeight.Bold),
    mulishVariable(FontWeight.ExtraBold),
    mulishVariable(FontWeight.Black),
    mulishVariable(ExtraBlack),
)

private val lineHeightStyle = LineHeightStyle(
    alignment = LineHeightStyle.Alignment.Center,
    trim = LineHeightStyle.Trim.None
)
private val platformTextStyle = PlatformTextStyle(includeFontPadding = false)

object NovaGameTypography {
    val waitingRoomHeadline = TextStyle(
        fontFamily = mulish,
        fontWeight = ExtraBlack,
        fontSize = 32.sp,
        lineHeight = 38.sp,
        lineHeightStyle = lineHeightStyle,
        platformStyle = platformTextStyle,
    )

    val countdown = TextStyle(
        fontFamily = mulish,
        fontWeight = ExtraBlack,
        fontSize = 164.sp,
        lineHeight = 164.sp,
        letterSpacing = 6.56.sp,
        lineHeightStyle = lineHeightStyle,
        platformStyle = platformTextStyle,
    )

    val howToPlay = TextStyle(
        fontFamily = mulish,
        fontWeight = ExtraBlack,
        fontSize = 22.sp,
        lineHeight = 28.sp,
        letterSpacing = 0.44.sp,
        lineHeightStyle = lineHeightStyle,
        platformStyle = platformTextStyle,
    )

    val pillText = TextStyle(
        fontFamily = mulish,
        fontWeight = ExtraBlack,
        fontSize = 18.sp,
        lineHeight = 22.sp,
        letterSpacing = 0.36.sp,
        lineHeightStyle = lineHeightStyle,
        platformStyle = platformTextStyle,
    )

    val prizesCardTopLabel = TextStyle(
        fontFamily = mulish,
        fontWeight = ExtraBlack,
        fontSize = 18.sp,
        lineHeight = 22.sp,
        letterSpacing = 0.36.sp,
        lineHeightStyle = lineHeightStyle,
        platformStyle = platformTextStyle,
    )

    val prizesCardMainText = TextStyle(
        fontFamily = mulish,
        fontWeight = ExtraBlack,
        fontSize = 32.sp,
        lineHeight = 40.sp,
        lineHeightStyle = lineHeightStyle,
        platformStyle = platformTextStyle,
    )

    val confirmButton = TextStyle(
        fontFamily = mulish,
        fontWeight = FontWeight.Black,
        fontSize = 22.sp,
        lineHeight = 28.sp,
        letterSpacing = 0.44.sp,
        textAlign = TextAlign.Center,
        lineHeightStyle = lineHeightStyle,
        platformStyle = platformTextStyle,
    )

    val votingHeader = TextStyle(
        fontFamily = mulish,
        fontWeight = FontWeight.Black,
        fontSize = 40.sp,
        lineHeight = 52.sp,
        textAlign = TextAlign.Center,
        lineHeightStyle = lineHeightStyle,
        platformStyle = platformTextStyle,
    )

    val hostIntroduction = TextStyle(
        fontFamily = mulish,
        fontWeight = FontWeight.Black,
        fontSize = 40.sp,
        lineHeight = 40.sp,
        lineHeightStyle = lineHeightStyle,
        platformStyle = platformTextStyle,
    )

    val connectionBadgeLabel = TextStyle(
        fontFamily = PolkadotFontFamilies.inter,
        fontWeight = FontWeight.SemiBold,
        fontSize = 12.sp,
        lineHeight = 20.sp,
        letterSpacing = 0.24.sp,
        lineHeightStyle = lineHeightStyle,
        platformStyle = platformTextStyle,
    )

    val connectingCountdown = TextStyle(
        fontFamily = PolkadotFontFamilies.inter,
        fontWeight = FontWeight.SemiBold,
        fontSize = 18.sp,
        lineHeight = 22.sp,
        lineHeightStyle = lineHeightStyle,
        platformStyle = platformTextStyle,
    )
}
