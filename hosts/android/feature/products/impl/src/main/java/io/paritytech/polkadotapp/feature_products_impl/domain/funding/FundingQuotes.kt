package io.paritytech.polkadotapp.feature_products_impl.domain.funding

import io.paritytech.polkadotapp.chains.network.binding.Balance
import io.paritytech.polkadotapp.common.utils.logFailure
import kotlinx.coroutines.channels.BufferOverflow
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableSharedFlow
import uniffi.truapi.FundingQuoteList
import uniffi.truapi.ProductId
import javax.inject.Inject
import javax.inject.Singleton

/** The quotes of each session: what the user asked to be quoted, and the core's list for the latest ask. */
interface FundingQuotes {
    /** Asks every provider for the reported selection to quote [amount], or drops the quotes on screen when `null`. */
    fun quote(amount: Balance?, intentId: FundingIntentId)
}

/** The latest ask on a session and what came of it. */
data class FundingQuotesShown(
    val amount: Balance?,
    /** The core's list for [amount]; `null` from a new ask until its first list. */
    val list: FundingQuoteList?,
    /** The ask did not reach the core, or its list could not be read. */
    val failed: Boolean,
)

/**
 * Keeps each session's asks and matches the core's lists to the latest one. Asks and read-backs run on the shared
 * [FundingCoreCalls] queue, so a list is only ever matched against the ask the core last answered.
 */
@Singleton
class RealFundingQuotes @Inject constructor(
    private val core: FundingCore,
    private val calls: FundingCoreCalls,
) : FundingQuotes {
    private val lock = Any()
    private val asks = mutableMapOf<FundingIntentId, QuoteAsk>()
    private val changes = MutableSharedFlow<FundingIntentId>(extraBufferCapacity = CHANGE_BUFFER, onBufferOverflow = BufferOverflow.DROP_OLDEST)

    /** The sessions whose quotes changed, as they do. Never ends. */
    val updates: Flow<FundingIntentId> = changes

    override fun quote(amount: Balance?, intentId: FundingIntentId) {
        if (amount == null) {
            synchronized(lock) { asks[intentId]?.drop() }
            changes.tryEmit(intentId)
            return
        }

        val number = synchronized(lock) { asks.getOrPut(intentId) { QuoteAsk() }.ask(amount) }
        changes.tryEmit(intentId)
        calls.launch {
            val requested = core.requestQuote(intentId, amount).logFailure("Funding quote request failed")
            val startsNow = synchronized(lock) {
                val ask = asks[intentId]
                requested.fold(onSuccess = { ask?.take(it, number) }, onFailure = { ask?.fail(number) }) == true
            }
            readList(intentId)
            if (startsNow) startWhenQuoted(intentId)
        }
    }

    /** What is shown for [intentId]'s latest ask; `null` while it has none. */
    fun shown(intentId: FundingIntentId): FundingQuotesShown? = synchronized(lock) { asks[intentId]?.shown() }

    /** The core says [intentId]'s list changed. */
    fun onQuotesChanged(intentId: FundingIntentId) {
        calls.launch { readList(intentId) }
    }

    /** The quote [provider] gave for the list on screen. */
    fun quoteId(intentId: FundingIntentId, provider: ProductId): String? = synchronized(lock) {
        asks[intentId]?.list?.rows?.firstOrNull { it.providerId == provider }?.state?.quote?.quoteId
    }

    /** Hands the session to the latest ask's best quote, now and for any ask after it, once every provider answered. */
    suspend fun startOnBest(intentId: FundingIntentId) {
        synchronized(lock) { asks.getOrPut(intentId) { QuoteAsk() }.startsWhenQuoted = true }
        calls.run { startWhenQuoted(intentId) }
    }

    fun isStartingOnBest(intentId: FundingIntentId): Boolean = synchronized(lock) { asks[intentId]?.startsWhenQuoted == true }

    /** The core started the session on a best quote, so nothing is armed any more. */
    fun startedOnBest(intentId: FundingIntentId) {
        synchronized(lock) { asks[intentId]?.startsWhenQuoted = false }
    }

    /** The user left the screen that started the session on its best quote: the core no longer starts it. */
    fun stopStarting(intentId: FundingIntentId) {
        val wasStarting = synchronized(lock) { asks[intentId]?.stopStarting() ?: false }
        if (wasStarting) calls.launch { core.cancelAutoStart(intentId).logFailure("Funding auto-start cancel failed") }
    }

    /** Runs on the call queue. */
    private suspend fun startWhenQuoted(intentId: FundingIntentId) {
        val requestId = synchronized(lock) { asks[intentId]?.armedRequest() } ?: return
        core.startWhenQuoted(intentId, requestId).logFailure("Funding auto-start failed")
    }

    /** Runs on the call queue. */
    private suspend fun readList(intentId: FundingIntentId) {
        val read = core.quotes(intentId).logFailure("Funding quotes read failed")
        synchronized(lock) {
            val ask = asks[intentId]
            read.fold(onSuccess = { ask?.receive(it) }, onFailure = { ask?.readFailed() })
        }
        changes.tryEmit(intentId)
    }

    /** The asks of one session: only the latest one's list is shown. Guarded by the lock. */
    private class QuoteAsk {
        private var number = 0

        /** The core's id for the latest ask, once it returned it. */
        private var requestId: ULong? = null

        /** The ask the core was last told to start the session on. */
        private var startedOn: ULong? = null

        private var awaiting = false

        private var failed = false

        private var amount: Balance? = null

        var list: FundingQuoteList? = null
            private set

        /** The session is handed to the latest ask's best quote once every provider answered. */
        var startsWhenQuoted = false

        fun shown(): FundingQuotesShown? = amount?.let { FundingQuotesShown(it, list, failed) }

        fun ask(asked: Balance): Int {
            amount = asked
            list = null
            awaiting = true
            failed = false
            requestId = null
            return ++number
        }

        fun drop() {
            number++
            amount = null
            list = null
            awaiting = false
            failed = false
            requestId = null
        }

        /**
         * Takes the core's id for ask [ofAsk] unless a later ask replaced it; `null` is the core naming no asset for
         * the selection, so nothing is asked. Answers whether to start on it now.
         */
        fun take(id: ULong?, ofAsk: Int): Boolean {
            if (ofAsk != number) return false
            if (id == null) {
                drop()
                return false
            }
            requestId = id
            return startsWhenQuoted
        }

        /** Ask [ofAsk] did not reach the core: it stays on screen as failed. Answers that nothing starts on it. */
        fun fail(ofAsk: Int): Boolean {
            if (ofAsk == number) {
                awaiting = false
                failed = true
            }
            return false
        }

        /**
         * The core's latest list: shown when it answers the latest ask. None while an ask waits for its first list is
         * the list not being there yet; none after it is a selection having withdrawn it.
         */
        fun receive(received: FundingQuoteList?) {
            if (received == null) {
                if (!awaiting && !failed) {
                    amount = null
                    list = null
                }
                return
            }
            if (received.requestId != requestId) return

            list = received
            awaiting = false
            failed = false
        }

        /** The list could not be read: the ask stays on screen as failed until a read succeeds. */
        fun readFailed() {
            if (amount == null) return
            awaiting = false
            failed = true
        }

        /** The request to start on, once per request. */
        fun armedRequest(): ULong? {
            val request = requestId?.takeIf { startsWhenQuoted && it != startedOn } ?: return null
            startedOn = request
            return request
        }

        fun stopStarting(): Boolean {
            val was = startsWhenQuoted
            startsWhenQuoted = false
            startedOn = null
            return was
        }
    }

    private companion object {
        const val CHANGE_BUFFER = 256
    }
}
