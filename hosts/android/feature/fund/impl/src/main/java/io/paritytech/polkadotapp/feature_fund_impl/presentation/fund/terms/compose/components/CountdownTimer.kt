package io.paritytech.polkadotapp.feature_fund_impl.presentation.fund.terms.compose.components

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.common.presentation.formatters.time.LocalTimeFormatter
import io.paritytech.polkadotapp.common.presentation.formatters.time.TimeFormatter
import io.paritytech.polkadotapp.design.components.progress.NovaCircularProgressIndicator
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import kotlinx.coroutines.delay
import kotlin.time.Duration
import kotlin.time.Duration.Companion.seconds

private val UPDATE_INTERVAL = 1.seconds

@Composable
fun CountdownTimer(
    duration: Duration,
    modifier: Modifier = Modifier,
    onFinished: () -> Unit = {},
) {
    Box(
        modifier = modifier,
        contentAlignment = Alignment.Center
    ) {
        var remaining by remember(duration) {
            mutableStateOf(duration.coerceAtLeast(Duration.ZERO))
        }

        val textPrimaryColor = PolkadotTheme.colors.fg.primary

        LaunchedEffect(duration) {
            remaining = duration.coerceAtLeast(Duration.ZERO)
            while (remaining > Duration.ZERO) {
                delay(UPDATE_INTERVAL)
                remaining = (remaining - UPDATE_INTERVAL).coerceAtLeast(Duration.ZERO)
            }
            onFinished.invoke()
        }

        val secondsLeft = remaining.coerceAtLeast(Duration.ZERO)

        val timeFormatter = LocalTimeFormatter.current

        NovaText(
            modifier = Modifier.padding(start = 2.dp),
            text = timeFormatter.formatTimeLeft(secondsLeft),
            color = textPrimaryColor,
            style = PolkadotTheme.typography.body.small,
        )

        NovaCircularProgressIndicator()
    }
}

@Preview
@Composable
fun PreviewCircularCountdown() {
    PolkadotTheme {
        CompositionLocalProvider(
            LocalTimeFormatter provides TimeFormatter.mocked(LocalContext.current)
        ) {
            CountdownTimer(
                modifier = Modifier.size(40.dp),
                duration = 10.seconds
            )
        }
    }
}
