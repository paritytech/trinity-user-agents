package io.paritytech.polkadotapp.feature_calls_impl.state

import io.paritytech.polkadotapp.feature_calls_api.domain.CallStateTracker
import io.paritytech.polkadotapp.feature_calls_api.domain.OfferId
import io.paritytech.polkadotapp.feature_calls_api.domain.models.CallDirection
import io.paritytech.polkadotapp.feature_calls_api.domain.models.CallStatus
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.tools_media_connection_api.domain.models.MediaState
import io.paritytech.polkadotapp.tools_media_connection_api.domain.models.MediaTracks
import kotlinx.coroutines.flow.StateFlow

interface CallStateHolder : CallStateTracker {
    fun initCall(chatId: ChatId, offerId: OfferId, direction: CallDirection, initiatedWithVideo: Boolean)
    fun updateStatus(status: CallStatus)
    fun updateMediaTracks(tracks: MediaTracks)
    fun observeMediaTracks(): StateFlow<MediaTracks>
    fun updateMediaState(state: MediaState)
    fun clear()
}
