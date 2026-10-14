package io.paritytech.polkadotapp.feature_chats_impl.presentation.feed.compose.components.messages.components

import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import kotlinx.coroutines.delay
import java.text.BreakIterator
import java.text.StringCharacterIterator
import kotlin.time.Duration
import kotlin.time.Duration.Companion.milliseconds

val REVEAL_TEXT_START_DELAY: Duration = 150.milliseconds
val REVEAL_MEDIA_START_DELAY: Duration = 600.milliseconds
val REVEAL_BETWEEN_MESSAGES_PAUSE: Duration = 250.milliseconds
private val REVEAL_PER_CHARACTER: Duration = 33.milliseconds

@Composable
fun rememberRevealedCharCount(
    fullText: String,
    isRevealing: Boolean,
    onRevealComplete: () -> Unit,
    startDelay: Duration = REVEAL_TEXT_START_DELAY,
    postRevealPause: Duration = REVEAL_BETWEEN_MESSAGES_PAUSE,
): Int {
    var revealedCount by remember(fullText, isRevealing) {
        mutableIntStateOf(if (isRevealing) 0 else fullText.length)
    }

    LaunchedEffect(fullText, isRevealing) {
        if (!isRevealing) {
            revealedCount = fullText.length
            return@LaunchedEffect
        }

        val iterator = BreakIterator.getCharacterInstance().apply { text = StringCharacterIterator(fullText) }
        delay(startDelay)
        var next = iterator.next()
        while (next != BreakIterator.DONE) {
            revealedCount = next
            next = iterator.next()
            delay(REVEAL_PER_CHARACTER)
        }
        if (postRevealPause > Duration.ZERO) delay(postRevealPause)
        onRevealComplete()
    }

    return revealedCount
}
