package io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest

import io.paritytech.polkadotapp.common.utils.coerceToUnit
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_account_api.domain.model.MetaAccount
import io.paritytech.polkadotapp.feature_account_api.domain.model.SharedSecretDerivationDomain
import io.paritytech.polkadotapp.feature_chats_impl.data.chatRequest.model.ChatRequestDecrypted
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ChatRequestRepository
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.transport.ChatRequestTopic
import io.paritytech.polkadotapp.feature_chats_impl.domain.chatRequest.transport.ChatRequestTransport
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.flatMapLatest
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.map
import timber.log.Timber
import javax.inject.Inject
import javax.inject.Singleton
import kotlin.time.Duration.Companion.minutes

/**
 * Service for discovering incoming chat requests.
 *
 * Implements two-stage discovery:
 * 1. Sync stage: Fetches historical requests from full topic on startup
 * 2. Poll stage: Continuously polls day topic for new requests
 */
interface ChatRequestDiscoveryService {
    /**
     * Performs sync and discovery of chat requests.
     * Syncs historical requests first, then polls for current day.
     *
     * @param metaAccount The account to discover requests for
     * @param domain The shared secret derivation domain for decryption
     */
    suspend fun discoverRequests(metaAccount: MetaAccount, domain: SharedSecretDerivationDomain)
}

@Singleton
class RealChatRequestDiscoveryService @Inject constructor(
    private val chatRequestTransport: ChatRequestTransport,
    private val chatRequestRepository: ChatRequestRepository,
    private val processor: IncomingChatRequestProcessor,
) : ChatRequestDiscoveryService {
    companion object {
        private val DAY_CHANGE_CHECK_INTERVAL = 1.minutes

        private const val MAX_DAYS_TO_SYNC = 7
    }

    override suspend fun discoverRequests(metaAccount: MetaAccount, domain: SharedSecretDerivationDomain) {
        performSync(metaAccount, domain)

        subscribeNewChatRequests(metaAccount, domain)
    }

    private suspend fun performSync(metaAccount: MetaAccount, domain: SharedSecretDerivationDomain) {
        val lastSyncedDay = chatRequestRepository.getLastSyncedDay(metaAccount.id)
        val currentDay = ChatRequestTopicDerivation.getCurrentDay()

        when {
            lastSyncedDay == null -> {
                logForAccount("First time sync - starting full topic sync", metaAccount)

                syncFromFullTopic(metaAccount, domain)
            }

            currentDay - lastSyncedDay > MAX_DAYS_TO_SYNC -> {
                logForAccount("Too may days has passed since last sync - full topic sync", metaAccount)

                syncFromFullTopic(metaAccount, domain)
            }

            else -> {
                logForAccount("Already synced up to day $lastSyncedDay. Starting per-day sync", metaAccount)

                syncDays(lastSyncedDay, currentDay, metaAccount, domain)
            }
        }
    }

    @OptIn(ExperimentalCoroutinesApi::class)
    private suspend fun subscribeNewChatRequests(
        metaAccount: MetaAccount,
        domain: SharedSecretDerivationDomain
    ) {
        currentDayTicker()
            .flatMapLatest { day ->
                val dayTopic = ChatRequestTopic.Day(acceptor = metaAccount.defaultAccountId(), day)

                chatRequestTransport.subscribeChatRequests(dayTopic, domain).map { result -> day to result }
            }
            .collect { (day, result) ->
                result
                    .onSuccess { requests ->
                        processRequests(requests, metaAccount, domain)
                        chatRequestRepository.updateLastSyncedDay(metaAccount.id, day)
                    }
                    .onFailure { warnForAccount("Failed to subscribe to $day day", metaAccount, it) }
            }
    }

    private fun currentDayTicker(): Flow<Long> = flow {
        while (true) {
            emit(ChatRequestTopicDerivation.getCurrentDay())

            delay(DAY_CHANGE_CHECK_INTERVAL)
        }
    }.distinctUntilChanged()

    private suspend fun syncFromFullTopic(
        metaAccount: MetaAccount,
        domain: SharedSecretDerivationDomain
    ) {
        val fullTopic = ChatRequestTopic.Full(acceptor = metaAccount.defaultAccountId())

        fetchAndProcessRequests(fullTopic, domain, metaAccount)
            .recover {
                warnForAccount("Full topic sync failed, fallbacking to recent $MAX_DAYS_TO_SYNC days sync", metaAccount, it)

                syncRecentDays(metaAccount, domain)
            }.onSuccess {
                val currentDay = ChatRequestTopicDerivation.getCurrentDay()
                chatRequestRepository.updateLastSyncedDay(metaAccount.id, currentDay)
            }
    }

    private suspend fun syncRecentDays(metaAccount: MetaAccount, domain: SharedSecretDerivationDomain) {
        val current = ChatRequestTopicDerivation.getCurrentDay()
        syncDays(current - MAX_DAYS_TO_SYNC, current, metaAccount, domain)
    }

    private suspend fun syncDays(
        startDay: Long,
        currentDay: Long,
        metaAccount: MetaAccount,
        domain: SharedSecretDerivationDomain
    ) {
        for (day in (startDay + 1)..currentDay) {
            syncFromDayTopic(metaAccount, domain, day)
        }
    }

    private suspend fun syncFromDayTopic(
        metaAccount: MetaAccount,
        domain: SharedSecretDerivationDomain,
        day: Long
    ): Result<Unit> {
        val dayTopic = ChatRequestTopic.Day(acceptor = metaAccount.defaultAccountId(), day)

        return fetchAndProcessRequests(dayTopic, domain, metaAccount)
            .onSuccess {
                chatRequestRepository.updateLastSyncedDay(metaAccount.id, day)
            }.onFailure {
                warnForAccount("Failed to sync $day day", metaAccount, it)
            }
    }

    private suspend fun fetchAndProcessRequests(
        topic: ChatRequestTopic,
        domain: SharedSecretDerivationDomain,
        acceptor: MetaAccount
    ): Result<Unit> {
        return chatRequestTransport.fetchChatRequests(topic, domain)
            .onSuccess { statements -> processRequests(statements, acceptor, domain) }
            .coerceToUnit()
    }

    private suspend fun processRequests(
        requests: List<ChatRequestDecrypted>,
        metaAccount: MetaAccount,
        derivationDomain: SharedSecretDerivationDomain,
    ) {
        for (request in requests) {
            val requestData = IncomingChatRequestData(request, metaAccount.id, derivationDomain)
            processor.processIncomingRequest(requestData)
                .logFailure("Failed to process request: $request")
        }
    }

    private fun logForAccount(log: String, account: MetaAccount) {
        Timber.d("$log for account ${account.name} (${account.purpose})")
    }

    private fun warnForAccount(log: String, account: MetaAccount, throwable: Throwable) {
        Timber.w(throwable, "$log for account ${account.name} (${account.purpose})")
    }
}
