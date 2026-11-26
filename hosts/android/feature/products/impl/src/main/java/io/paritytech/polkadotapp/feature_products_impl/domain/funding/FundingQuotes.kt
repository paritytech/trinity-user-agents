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

interface FundingQuotes {
    // Sessions whose quotes changed. Never ends.
    val updates: Flow<FundingIntentId>

    // `null` drops the quotes on screen.
    fun quote(amount: Balance?, intentId: FundingIntentId)

    fun latestAsk(intentId: FundingIntentId): FundingQuotesShown?

    fun quoteId(intentId: FundingIntentId, provider: ProductId): String?

    // Starts on the best quote of the latest ask, and of every ask after it, once every provider answered.
    suspend fun startOnBest(intentId: FundingIntentId): Result<Unit>

    // The session's screens are gone: drops its asks and any start on the best quote, on this side only.
    fun forget(intentId: FundingIntentId)
}

data class FundingQuotesShown(
    val amount: Balance?,
    // `null` from a new ask until its first list.
    val list: FundingQuoteList?,
    val failed: Boolean,
)

// Asks and read-backs share the FundingCoreCalls queue, so a list is only matched against the ask the core last answered.
@Singleton
class RealFundingQuotes @Inject constructor(
    private val core: FundingCore,
    private val calls: FundingCoreCalls,
) : FundingQuotes {
    private val lock = Any()
    private val asks = mutableMapOf<FundingIntentId, QuoteAsk>()
    private val changes = MutableSharedFlow<FundingIntentId>(extraBufferCapacity = CHANGE_BUFFER, onBufferOverflow = BufferOverflow.DROP_OLDEST)

    override val updates: Flow<FundingIntentId> = changes

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
            if (startsNow) startWhenQuoted(intentId).logFailure("Funding auto-start failed")
        }
    }

    override fun latestAsk(intentId: FundingIntentId): FundingQuotesShown? = synchronized(lock) { asks[intentId]?.shown() }

    fun onQuotesChanged(intentId: FundingIntentId) {
        calls.launch { readList(intentId) }
    }

    override fun quoteId(intentId: FundingIntentId, provider: ProductId): String? = synchronized(lock) {
        asks[intentId]?.list?.rows?.firstOrNull { it.providerId == provider }?.state?.quote?.quoteId
    }

    override suspend fun startOnBest(intentId: FundingIntentId): Result<Unit> {
        synchronized(lock) { asks.getOrPut(intentId) { QuoteAsk() }.startsWhenQuoted = true }
        return calls.run { startWhenQuoted(intentId) }
    }

    override fun forget(intentId: FundingIntentId) {
        synchronized(lock) { asks.remove(intentId) }
    }

    // Runs on the call queue. A failed start leaves nothing armed, so a later ask does not start the session unasked.
    private suspend fun startWhenQuoted(intentId: FundingIntentId): Result<Unit> {
        val requestId = synchronized(lock) { asks[intentId]?.armedRequest() } ?: return Result.success(Unit)
        return core.startWhenQuoted(intentId, requestId).onFailure {
            synchronized(lock) { asks[intentId]?.stopStarting() }
        }
    }

    // Runs on the call queue.
    private suspend fun readList(intentId: FundingIntentId) {
        val read = core.quotes(intentId).logFailure("Funding quotes read failed")
        synchronized(lock) {
            val ask = asks[intentId]
            read.fold(onSuccess = { ask?.receive(it) }, onFailure = { ask?.readFailed() })
        }
        changes.tryEmit(intentId)
    }

    // Guarded by the lock. Only the latest ask's list is shown.
    private class QuoteAsk {
        private var number = 0

        private var requestId: ULong? = null

        // The request the core was last told to start the session on.
        private var startedOn: ULong? = null

        private var awaiting = false

        private var failed = false

        private var amount: Balance? = null

        var list: FundingQuoteList? = null
            private set

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

        // A `null` id is the core naming no asset for the selection, so nothing was asked. Answers whether to start now.
        fun take(id: ULong?, ofAsk: Int): Boolean {
            if (ofAsk != number) return false
            if (id == null) {
                drop()
                return false
            }
            requestId = id
            return startsWhenQuoted
        }

        fun fail(ofAsk: Int): Boolean {
            if (ofAsk == number) {
                awaiting = false
                failed = true
            }
            return false
        }

        // No list while an ask awaits its first one means it is not there yet; after that, a selection withdrew it.
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

        fun readFailed() {
            if (amount == null) return
            awaiting = false
            failed = true
        }

        fun armedRequest(): ULong? {
            val request = requestId?.takeIf { startsWhenQuoted && it != startedOn } ?: return null
            startedOn = request
            return request
        }

        fun stopStarting() {
            startsWhenQuoted = false
            startedOn = null
        }
    }

    private companion object {
        const val CHANGE_BUFFER = 256
    }
}
