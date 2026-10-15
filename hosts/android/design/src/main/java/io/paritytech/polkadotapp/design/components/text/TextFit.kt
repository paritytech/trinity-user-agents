package io.paritytech.polkadotapp.design.components.text

import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.TextLayoutResult
import androidx.compose.ui.text.TextMeasurer
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.TextUnit
import androidx.compose.ui.unit.sp

private const val FONT_SIZE_FIT_STEPS = 8

fun TextMeasurer.fitOnOneLine(
    text: AnnotatedString,
    style: TextStyle,
    minFontSize: TextUnit,
    maxWidth: Int,
): TextLayoutResult {
    require(style.fontSize.isSp && minFontSize.isSp && minFontSize <= style.fontSize)

    val constraints = Constraints(maxWidth = maxWidth)

    fun measureAt(fontSize: Float) =
        measure(text, style.copy(fontSize = fontSize.sp), softWrap = false, maxLines = 1, constraints = constraints)

    val atMax = measureAt(style.fontSize.value)
    if (!atMax.didOverflowWidth) return atMax

    val atMin = measureAt(minFontSize.value)
    if (atMin.didOverflowWidth) return atMin

    var fits = atMin
    var low = minFontSize.value
    var high = style.fontSize.value
    repeat(FONT_SIZE_FIT_STEPS) {
        val mid = (low + high) / 2
        val candidate = measureAt(mid)
        if (candidate.didOverflowWidth) {
            high = mid
        } else {
            low = mid
            fits = candidate
        }
    }

    return fits
}
