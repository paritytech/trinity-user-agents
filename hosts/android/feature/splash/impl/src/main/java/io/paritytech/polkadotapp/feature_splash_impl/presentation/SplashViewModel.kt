package io.paritytech.polkadotapp.feature_splash_impl.presentation

import dagger.hilt.android.lifecycle.HiltViewModel
import io.paritytech.polkadotapp.common.presentation.screens.BaseViewModel
import io.paritytech.polkadotapp.feature_splash_api.presentation.SplashPassedObserver
import io.paritytech.polkadotapp.feature_splash_impl.domain.SplashInteractor
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import javax.inject.Inject
import kotlin.time.Duration.Companion.seconds

@HiltViewModel
class SplashViewModel @Inject constructor(
    private val interactor: SplashInteractor,
    private val router: SplashRouter,
    private val splashPassedObserver: SplashPassedObserver,
) : BaseViewModel(), SplashContract {
    val waitingForNetworkVisible: StateFlow<Boolean>
        field = MutableStateFlow(false)

    init {
        launch { interactor.syncRemoteConfigWhenOnline() }

        val timeoutJob = launch {
            delay(5.seconds)
            waitingForNetworkVisible.value = true
        }

        launch {
            val status = interactor.observeAccountOnboardingStatus()
                .first()

            when {
                status.isOnboarded -> router.openMain()
                status.isWaitingInQueue -> router.openRegistrationQueue()
                status.accountCreated -> router.openClaimUsername()
                else -> router.openThemeSelection()
            }

            splashPassedObserver.setSplashPassed()

            timeoutJob.cancel()
            waitingForNetworkVisible.value = false
        }
    }
}
