package io.paritytech.polkadotapp.design.utils

import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight

@Composable
fun String.withBold(text: String): AnnotatedString = remember(this, text) {
    buildAnnotatedString {
        append(this@withBold)

        val startIndex = indexOf(text)
        val endIndex = startIndex + text.length

        addStyle(
            style = SpanStyle(fontWeight = FontWeight.Bold),
            start = startIndex,
            end = endIndex
        )
    }
}
