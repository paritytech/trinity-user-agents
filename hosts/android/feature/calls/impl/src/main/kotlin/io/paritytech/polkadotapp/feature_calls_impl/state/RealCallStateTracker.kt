package io.paritytech.polkadotapp.feature_calls_impl.state

import io.paritytech.polkadotapp.feature_calls_api.domain.OfferId
import io.paritytech.polkadotapp.feature_calls_api.domain.models.ActiveCallState
import io.paritytech.polkadotapp.feature_calls_api.domain.models.CallDirection
import io.paritytech.polkadotapp.feature_calls_api.domain.models.CallStatus
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.tools_media_connection_api.domain.models.MediaState
import io.paritytech.polkadotapp.tools_media_connection_api.domain.models.MediaTracks
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class RealCallStateTracker @Inject constructor() : CallStateHolder {
    private val activeCall = MutableStateFlow<ActiveCallState?>(null)
    private val mediaTracks = MutableStateFlow(MediaTracks())
    private val mediaState = MutableStateFlow(MediaState())

    override fun observeActiveCall(): StateFlow<ActiveCallState?> = activeCall.asStateFlow()
    override fun getActiveCall(): ActiveCallState? = activeCall.value

    override fun initCall(chatId: ChatId, offerId: OfferId, direction: CallDirection, initiatedWithVideo: Boolean) {
        val initialStatus = when (direction) {
            CallDirection.Outgoing -> CallStatus.Requesting
            CallDirection.Incoming -> CallStatus.Ringing
        }
        activeCall.value = ActiveCallState(chatId, offerId, direction, initialStatus, initiatedWithVideo)
    }

    override fun updateStatus(status: CallStatus) {
        activeCall.update { it?.copy(status = status) }
    }

    override fun updateMediaTracks(tracks: MediaTracks) { mediaTracks.value = tracks }
    override fun observeMediaTracks(): StateFlow<MediaTracks> = mediaTracks.asStateFlow()

    override fun updateMediaState(state: MediaState) { mediaState.value = state }
    override fun observeMediaState(): StateFlow<MediaState> = mediaState.asStateFlow()

    override fun clear() {
        activeCall.value = null
        mediaTracks.value = MediaTracks()
        mediaState.value = MediaState()
    }
}
