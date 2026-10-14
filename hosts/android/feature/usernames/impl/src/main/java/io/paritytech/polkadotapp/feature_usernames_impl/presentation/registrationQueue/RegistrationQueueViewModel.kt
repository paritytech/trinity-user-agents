package io.paritytech.polkadotapp.feature_usernames_impl.presentation.registrationQueue

import androidx.compose.runtime.Immutable
import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.presentation.loading.LoadingState
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.common.utils.shareInBackground
import io.paritytech.polkadotapp.common.utils.withLoading
import io.paritytech.polkadotapp.feature_usernames_impl.domain.registrationQueue.RegistrationQueueInteractor
import io.paritytech.polkadotapp.feature_usernames_impl.domain.registrationQueue.RegistrationQueueProgress
import io.paritytech.polkadotapp.feature_usernames_impl.presentation.UsernamesRouter
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.filterIsInstance
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import javax.inject.Inject

@HiltViewModel
class RegistrationQueueViewModel @Inject constructor(
    interactor: RegistrationQueueInteractor,
    private val router: UsernamesRouter,
) : BaseViewModel() {
    private val queueProgress = interactor.observeQueueProgress()
        .shareInBackground()

    val state: StateFlow<LoadingState<RegistrationQueueState>> =
        queueProgress
            .filterIsInstance<RegistrationQueueProgress.Waiting>()
            .map { it.toUiState() }
            .withLoading("RegistrationQueue")
            .stateIn(this, SharingStarted.Eagerly, LoadingState.Loading)

    init {
        observeQueueCompleted()
    }

    fun backPressed() {
        router.back()
    }

    private fun observeQueueCompleted() = launch {
        queueProgress.first { it is RegistrationQueueProgress.Completed }
        router.openMain()
    }
}

@Immutable
data class RegistrationQueueState(
    val position: Int,
    val progress: Float
)

private fun RegistrationQueueProgress.Waiting.toUiState(): RegistrationQueueState {
    val progress = if (initialPosition > 0) {
        (initialPosition - position).toFloat() / initialPosition
    } else {
        0f
    }
    return RegistrationQueueState(position = position, progress = progress.coerceIn(0f, 1f))
}
