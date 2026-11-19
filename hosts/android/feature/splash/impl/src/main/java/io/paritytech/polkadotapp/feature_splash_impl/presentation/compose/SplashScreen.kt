package io.paritytech.polkadotapp.feature_splash_impl.presentation.compose

import androidx.compose.animation.AnimatedContent
import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.progressSemantics
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.ColorFilter
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.res.vectorResource
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.components.text.NovaText
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_splash_impl.presentation.SplashViewModel
import io.paritytech.polkadotapp.feature_splash_impl.presentation.compose.components.icons.PolkadotLogo
import io.paritytech.polkadotapp.common.R as RCommon

@Composable
fun SplashScreen(viewModel: SplashViewModel) {
    val waitingForNetworkVisible by viewModel.waitingForNetworkVisible.collectAsStateWithLifecycle()

    PolkadotSurface {
        AnimatedContent(
            modifier = Modifier.fillMaxSize(),
            targetState = waitingForNetworkVisible
        ) { waitingForNetwork ->
            if (waitingForNetwork) {
                Column(
                    modifier = Modifier
                        .fillMaxSize()
                        .padding(PolkadotTheme.spacings.mediumIncreased),
                    horizontalAlignment = Alignment.CenterHorizontally,
                    verticalArrangement = Arrangement.Center
                ) {
                    Image(
                        imageVector = PolkadotLogo,
                        contentDescription = "logo",
                        colorFilter = ColorFilter.tint(PolkadotTheme.colors.fg.primary)
                    )

                    VerticalSpacer { mediumIncreased }

                    NovaText(
                        text = stringResource(RCommon.string.splash_waiting_for_network_title),
                        style = PolkadotTheme.typography.title.medium,
                        color = PolkadotTheme.colors.fg.secondary
                    )

                    VerticalSpacer { tiny }

                    NovaText(
                        text = stringResource(RCommon.string.splash_waiting_for_network_message),
                        style = PolkadotTheme.typography.body.medium,
                        color = PolkadotTheme.colors.fg.secondary
                    )
                }
            } else {
                Box(
                    modifier = Modifier
                        .fillMaxSize()
                        .progressSemantics(),
                    contentAlignment = Alignment.Center
                ) {
                    Image(
                        imageVector = ImageVector.vectorResource(RCommon.drawable.splash_logo),
                        contentDescription = null,
                        modifier = Modifier.size(SplashIconCanvasSize),
                        colorFilter = ColorFilter.tint(PolkadotTheme.colors.fg.primary)
                    )
                }
            }
        }
    }
}

// 288dp = system splash icon canvas, so the mark keeps its size and place at hand-over
private val SplashIconCanvasSize = 288.dp
