package io.paritytech.polkadotapp.feature_calls_impl.product

import io.paritytech.polkadotapp.feature_calls_api.domain.EncodedIceCandidates
import io.paritytech.polkadotapp.feature_calls_api.domain.EncodedSpd
import io.paritytech.polkadotapp.feature_calls_api.domain.ExternalCallSignaling
import io.paritytech.polkadotapp.feature_calls_api.domain.IncomingOffer
import io.paritytech.polkadotapp.feature_calls_api.domain.OfferId
import io.paritytech.polkadotapp.feature_calls_api.domain.ProductCallSignals
import io.paritytech.polkadotapp.feature_calls_api.domain.ProductMediaCall
import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatId
import io.paritytech.polkadotapp.tools_media_connection_api.domain.signaling.PeerSignal
import io.paritytech.polkadotapp.tools_media_connection_api.domain.signaling.PeerSignalCoder
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.receiveAsFlow
import timber.log.Timber

/** The signaling of one product call: the product carries what the engine sends and gives back what the other side sent. */
class ProductCallSession(
    val call: ProductMediaCall,
    private val signals: ProductCallSignals,
    private val coder: PeerSignalCoder,
    private val onClosed: (ProductCallSession) -> Unit,
) : ExternalCallSignaling {
    private val offer = CompletableDeferred<EncodedSpd>()
    private val answer = CompletableDeferred<EncodedSpd>()
    private val candidates = Channel<EncodedIceCandidates>(Channel.UNLIMITED)
    private val offerSent = MutableStateFlow(false)
    private val closed = MutableStateFlow(false)

    fun receive(signal: ByteArray) {
        val decoded = runCatching { coder.decode(signal) }.getOrElse {
            Timber.w(it, "Product call %s: a signal of the other side does not decode", call.callId)
            return
        }
        decoded.forEach { item ->
            when (item) {
                is PeerSignal.Offer -> offer.complete(item.setup)
                is PeerSignal.Answer -> answer.complete(item.setup)
                is PeerSignal.Candidates -> candidates.trySend(item.candidates)
                PeerSignal.Closed -> close()
            }
        }
    }

    /** Returns false when the call had already closed. */
    fun close(): Boolean {
        if (!closed.compareAndSet(expect = false, update = true)) return false
        onClosed(this)
        return true
    }

    override suspend fun sendOffer(offerId: OfferId, chatId: ChatId, sdp: EncodedSpd, withVideo: Boolean) {
        send(PeerSignal.Offer(sdp))
        offerSent.value = true
    }

    override suspend fun sendAnswer(offerId: OfferId, chatId: ChatId, sdp: EncodedSpd) = send(PeerSignal.Answer(sdp))

    override suspend fun sendIceCandidates(offerId: OfferId, chatId: ChatId, candidates: EncodedIceCandidates) =
        send(PeerSignal.Candidates(candidates))

    override fun subscribeIncomingIceCandidates(chatId: ChatId, offerId: OfferId): Flow<EncodedIceCandidates> =
        candidates.receiveAsFlow()

    override suspend fun awaitIncomingAnswer(offerId: OfferId, chatId: ChatId): EncodedSpd = answer.await()

    override suspend fun awaitIncomingOffer(offerId: OfferId): IncomingOffer = IncomingOffer(offer.await(), call.video)

    override suspend fun sendCloseSignal(offerId: OfferId, chatId: ChatId) {
        if (!close()) return
        send(PeerSignal.Closed)
        signals.ended()
    }

    override fun subscribeIncomingCloseSignal(offerId: OfferId, chatId: ChatId): Flow<Unit> =
        closed.filter { it }.map { }

    override fun observeOfferReadStatus(offerId: OfferId): Flow<Boolean> = offerSent.asStateFlow()

    private suspend fun send(signal: PeerSignal) = signals.send(coder.encode(signal))
}
