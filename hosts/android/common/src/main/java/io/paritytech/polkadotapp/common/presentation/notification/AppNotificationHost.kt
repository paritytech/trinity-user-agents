package io.paritytech.polkadotapp.common.presentation.notification

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.tooling.preview.Preview
import androidx.compose.ui.unit.dp
import io.paritytech.polkadotapp.common.presentation.compose.withCurrencyTickerStyle
import io.paritytech.polkadotapp.common.presentation.paymentAsset.LocalPaymentAssetBrand
import io.paritytech.polkadotapp.common.presentation.paymentAsset.PaymentAssetBrand
import io.paritytech.polkadotapp.design.components.icon.NovaIcon
import io.paritytech.polkadotapp.design.components.icon.NovaIcons
import io.paritytech.polkadotapp.design.components.icon.vectors.Alert
import io.paritytech.polkadotapp.design.components.icon.vectors.CheckCircleOutlined
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import kotlinx.coroutines.delay

private const val SUCCESS_DISPLAY_MS = 2_500L
private const val ERROR_DISPLAY_MS = 4_000L

@Composable
fun AppNotificationHost(notifier: AppNotifier) {
    var visible by remember { mutableStateOf<AppNotification?>(null) }

    LaunchedEffect(notifier) {
        notifier.notifications.collect { notification ->
            visible = notification
            val displayMs = when (notification) {
                is AppNotification.Success -> SUCCESS_DISPLAY_MS
                is AppNotification.Error -> ERROR_DISPLAY_MS
            }
            delay(displayMs)
            if (visible == notification) {
                visible = null
            }
        }
    }

    Box(modifier = Modifier.fillMaxSize()) {
        AnimatedVisibility(
            modifier = Modifier
                .align(Alignment.BottomCenter)
                .navigationBarsPadding()
                .padding(
                    horizontal = PolkadotTheme.spacings.mediumIncreased,
                    vertical = PolkadotTheme.spacings.large,
                ),
            visible = visible != null,
            enter = slideInVertically(initialOffsetY = { it }) + fadeIn(),
            exit = slideOutVertically(targetOffsetY = { it }) + fadeOut(),
        ) {
            visible?.let { AppNotificationItem(notification = it) }
        }
    }
}

@Composable
private fun AppNotificationItem(notification: AppNotification) {
    val backgroundColor = when (notification) {
        is AppNotification.Success -> PolkadotTheme.colors.bg.action.tertiary
        is AppNotification.Error -> PolkadotTheme.colors.fg.error
    }
    val iconTint = when (notification) {
        is AppNotification.Success -> PolkadotTheme.colors.fg.success
        is AppNotification.Error -> Color.White
    }
    val icon = when (notification) {
        is AppNotification.Success -> NovaIcons.CheckCircleOutlined
        is AppNotification.Error -> NovaIcons.Alert
    }

    PolkadotSurface(
        modifier = Modifier.fillMaxWidth(),
        color = backgroundColor,
        shape = RoundedCornerShape(28.dp),
    ) {
        Row(
            modifier = Modifier.padding(
                horizontal = PolkadotTheme.spacings.large,
                vertical = PolkadotTheme.spacings.mediumIncreased,
            ),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(PolkadotTheme.spacings.mediumIncreased),
        ) {
            NovaIcon(
                modifier = Modifier.size(24.dp),
                imageVector = icon,
                tint = iconTint,
            )

            NovaText(
                modifier = Modifier.weight(1f),
                text = notification.message.withCurrencyTickerStyle(PolkadotTheme.typography.title.small),
                color = PolkadotTheme.colors.fg.primary,
                style = PolkadotTheme.typography.title.small,
            )
        }
    }
}

@Preview
@Composable
private fun AppNotificationItemSuccessPreview() = CompositionLocalProvider(
    LocalPaymentAssetBrand provides PaymentAssetBrand.mocked
) {
    PolkadotTheme {
        AppNotificationItem(notification = AppNotification.Success("Device connected"))
    }
}

@Preview
@Composable
private fun AppNotificationItemErrorPreview() = CompositionLocalProvider(
    LocalPaymentAssetBrand provides PaymentAssetBrand.mocked
) {
    PolkadotTheme {
        AppNotificationItem(notification = AppNotification.Error("Something went wrong. Please try again."))
    }
}
