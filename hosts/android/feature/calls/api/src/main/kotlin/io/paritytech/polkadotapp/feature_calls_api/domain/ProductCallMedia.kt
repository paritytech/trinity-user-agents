package io.paritytech.polkadotapp.feature_calls_api.domain

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId

/**
 * Runs the media of a call of a product room (TrUAPI `callMedia`) on the call engine and the call screen of the
 * contact calls. The product carries the signals between the two devices. A signal is the SCALE
 * `Vec<PeerConnectionSignal>` of one send, with the SDP as text, as the iOS app encodes it.
 */
interface ProductCallMedia {
    /** Starts [call] and opens the call screen. Fails while another call runs or without the microphone permission. */
    fun start(call: ProductMediaCall, signals: ProductCallSignals): Result<Unit>

    /** A signal of the other side of [callId]. A signal that comes before its call starts waits for it. */
    fun signal(chatId: ChatId, callId: String, signal: ByteArray)

    /** Ends [callId], which the product ended. */
    fun stop(chatId: ChatId, callId: String)
}

data class ProductMediaCall(
    val chatId: ChatId,
    val callId: String,
    val roomName: String,
    val video: Boolean,
    val offerer: Boolean,
)

/** Where the call engine of a product call sends what the product carries. */
interface ProductCallSignals {
    /** A signal of the engine for the other side. */
    suspend fun send(signal: ByteArray)

    /** The user ended the call on the call screen. */
    suspend fun ended()
}
