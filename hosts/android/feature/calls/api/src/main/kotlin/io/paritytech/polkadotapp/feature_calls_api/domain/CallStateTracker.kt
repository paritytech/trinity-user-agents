package io.paritytech.polkadotapp.feature_calls_api.domain

import io.paritytech.polkadotapp.feature_calls_api.domain.models.ActiveCallState
import io.paritytech.polkadotapp.tools_media_connection_api.domain.models.MediaState
import kotlinx.coroutines.flow.Flow

interface CallStateTracker {
    fun observeActiveCall(): Flow<ActiveCallState?>
    fun getActiveCall(): ActiveCallState?

    fun observeMediaState(): Flow<MediaState>
}
