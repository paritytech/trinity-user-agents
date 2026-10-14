package io.paritytech.polkadotapp.feature_usernames_impl.presentation.registrationQueue.compose

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.systemBarsPadding
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.tooling.preview.Preview
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.design.components.bottomsheet.NovaModalBottomSheet
import io.paritytech.polkadotapp.design.components.progress.NovaCircularProgressIndicator
import io.paritytech.polkadotapp.design.components.spacer.FillerSpacer
import io.paritytech.polkadotapp.design.components.spacer.VerticalSpacer
import io.paritytech.polkadotapp.design.components.surface.PolkadotSurface
import io.paritytech.polkadotapp.design.theme.PolkadotTheme
import io.paritytech.polkadotapp.feature_usernames_impl.presentation.registrationQueue.RegistrationQueueState
import io.paritytech.polkadotapp.feature_usernames_impl.presentation.registrationQueue.RegistrationQueueViewModel
import io.paritytech.polkadotapp.feature_usernames_impl.presentation.registrationQueue.compose.components.QueueWaitingContent
import io.paritytech.polkadotapp.feature_usernames_impl.presentation.registrationQueue.compose.components.WhyQueueSheetContent
import io.paritytech.polkadotapp.feature_usernames_impl.presentation.registrationQueue.compose.components.WhyWaitingFooter

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun RegistrationQueueScreen(viewModel: RegistrationQueueViewModel) {
    val state by viewModel.state.collectAsStateWithLifecycle()
    var whySheetVisible by remember { mutableStateOf(false) }

    BackHandler { viewModel.backPressed() }

    RegistrationQueueScreenInternal(
        state = state,
        onWhyClicked = { whySheetVisible = true }
    )

    NovaModalBottomSheet(
        isVisible = whySheetVisible,
        onDismissRequest = { whySheetVisible = false }
    ) {
        WhyQueueSheetContent(onGotItClicked = { whySheetVisible = false })
    }
}

@Composable
private fun RegistrationQueueScreenInternal(
    state: LoadingState<RegistrationQueueState>,
    onWhyClicked: () -> Unit,
) {
    PolkadotSurface {
        Column(
            modifier = Modifier
                .fillMaxSize()
                .background(PolkadotTheme.colors.bg.surface.main)
                .systemBarsPadding()
                .padding(horizontal = PolkadotTheme.spacings.medium),
            horizontalAlignment = Alignment.CenterHorizontally
        ) {
            FillerSpacer()

            when (state) {
                is LoadingState.Loaded -> QueueWaitingContent(state.data)
                else -> NovaCircularProgressIndicator(color = PolkadotTheme.colors.fg.primary)
            }

            FillerSpacer()

            WhyWaitingFooter(onWhyClicked = onWhyClicked)

            VerticalSpacer { medium }
        }
    }
}

@Preview
@Composable
private fun RegistrationQueueScreenPreview() {
    PolkadotTheme {
        RegistrationQueueScreenInternal(
            state = LoadingState.Loaded(RegistrationQueueState(position = 400, progress = 0.35f)),
            onWhyClicked = {}
        )
    }
}
