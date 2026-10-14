package io.paritytech.polkadotapp.tools_media_connection_api.domain

import timber.log.Timber

/**
 * Tags peer-connection logs with a session id. The id is resolved on every call, so a caller whose
 * id only becomes known mid-handshake (e.g. a WebRTC offerId learned from the wire) can supply a
 * provider that returns null until then — the log line states why no id is available yet instead of
 * showing a placeholder value.
 */
class PeerConnectionLogger(
    private val sessionIdProvider: () -> String?
) {
    constructor(sessionId: String) : this({ sessionId })

    fun log(message: String, error: Throwable? = null) {
        val line = "[sessionId=${sessionIdProvider() ?: NO_SESSION_ID_YET}] $message"
        if (error != null) {
            Timber.tag(TAG).w(error, line)
        } else {
            Timber.tag(TAG).i(line)
        }
    }

    private companion object {
        const val TAG = "PeerChannelConnection"
        const val NO_SESSION_ID_YET = "none-yet: offer/answer not exchanged"
    }
}
