package io.paritytech.polkadotapp.common.presentation

import androidx.lifecycle.LifecycleObserver
import io.paritytech.polkadotapp.common.data.app.AppLifecycleState
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map

interface AppLifecycleObserver : LifecycleObserver {
    fun subscribe(): Flow<AppLifecycleState>
    fun getCurrentState(): AppLifecycleState
}

fun AppLifecycleObserver.subscribeIsForeground(): Flow<Boolean> =
    subscribe()
        .map { it == AppLifecycleState.FOREGROUND }
        .distinctUntilChanged()
