package io.paritytech.polkadotapp.feature_videogame_impl.presentation.compose.theme

import androidx.compose.ui.text.ExperimentalTextApi
import androidx.compose.ui.text.PlatformTextStyle
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontVariation
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.LineHeightStyle
import androidx.compose.ui.unit.sp
import io.paritytech.polkadotapp.feature_videogame_impl.R

private val ExtraBlack = FontWeight(1000)

@OptIn(ExperimentalTextApi::class)
private val mulishExtraBlack = FontFamily(
    Font(
        resId = R.font.mulish_variable,
        weight = ExtraBlack,
        variationSettings = FontVariation.Settings(FontVariation.weight(ExtraBlack.weight))
    )
)

object NovaGameTypography {
    val pillText = TextStyle(
        fontFamily = mulishExtraBlack,
        fontWeight = ExtraBlack,
        fontSize = 18.sp,
        lineHeight = 22.sp,
        letterSpacing = 0.36.sp,
        lineHeightStyle = LineHeightStyle(
            alignment = LineHeightStyle.Alignment.Center,
            trim = LineHeightStyle.Trim.None
        ),
        platformStyle = PlatformTextStyle(includeFontPadding = false),
    )
}
