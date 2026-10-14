package io.paritytech.polkadotapp.common.presentation.screens

import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.res.stringResource
import io.paritytech.polkadotapp.common.presentation.notification.AppNotifier
import io.paritytech.polkadotapp.common.presentation.notification.error
import io.paritytech.polkadotapp.common.presentation.notification.success

// The only collector of BaseViewModel.events: the channel is conflated and single-consumer, so a second one would steal events.
@Composable
fun ObserveViewModelEvents(viewModel: BaseViewModel, appNotifier: AppNotifier) {
    var pending by remember { mutableStateOf<BaseViewModelEvent?>(null) }

    LaunchedEffect(viewModel) {
        viewModel.events.collect { pending = it }
    }

    pending?.let { event ->
        val message = when (event) {
            is BaseViewModelEvent.PresentationError -> event.error.message()
            is BaseViewModelEvent.Error -> event.errorTitle
            is BaseViewModelEvent.Message -> event.message
            is BaseViewModelEvent.ResourceMessage -> stringResource(event.messageRes)
        }
        val isError = event is BaseViewModelEvent.PresentationError || event is BaseViewModelEvent.Error

        LaunchedEffect(event) {
            if (isError) {
                appNotifier.error(message)
            } else {
                appNotifier.success(message)
            }

            pending = null
        }
    }
}
