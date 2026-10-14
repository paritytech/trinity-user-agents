package io.paritytech.polkadotapp.feature_device_sync_impl.domain.signaling

import io.paritytech.polkadotapp.tools_media_connection_api.domain.signaling.SignalingMessage
import kotlinx.serialization.Serializable

typealias SyncOfferId = String

/**
 * SCALE wrapper for device-sync WebRTC signalling. [offerId] identifies the connection attempt a
 * [message] belongs to so peers can correlate answer/ICE and detect stale connections. No gameIndex —
 * each peer has its own CommunicationSession/topic. Wire layout must byte-match the desktop sync signaler.
 */
@Serializable
class SyncSignalingEnvelope(
    val offerId: SyncOfferId,
    val message: SignalingMessage
)
