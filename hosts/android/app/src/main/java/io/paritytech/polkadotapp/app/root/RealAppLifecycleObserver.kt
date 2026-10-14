package io.paritytech.polkadotapp.app.root

import androidx.lifecycle.DefaultLifecycleObserver
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleOwner
import androidx.lifecycle.ProcessLifecycleOwner
import io.paritytech.polkadotapp.common.data.app.AppLifecycleState
import io.paritytech.polkadotapp.common.presentation.AppLifecycleObserver
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class RealAppLifecycleObserver @Inject constructor() : DefaultLifecycleObserver, AppLifecycleObserver {
    private val appLifecycleStateFlow = MutableStateFlow(currentLifecycleState())

    override fun onStart(owner: LifecycleOwner) {
        appLifecycleStateFlow.value = AppLifecycleState.FOREGROUND
    }

    override fun onStop(owner: LifecycleOwner) {
        appLifecycleStateFlow.value = AppLifecycleState.BACKGROUND
    }

    override fun subscribe(): Flow<AppLifecycleState> {
        return appLifecycleStateFlow
    }

    override fun getCurrentState(): AppLifecycleState {
        return appLifecycleStateFlow.value
    }

    private fun currentLifecycleState(): AppLifecycleState {
        return if (ProcessLifecycleOwner.get().lifecycle.currentState.isAtLeast(Lifecycle.State.STARTED)) {
            AppLifecycleState.FOREGROUND
        } else {
            AppLifecycleState.BACKGROUND
        }
    }
}
