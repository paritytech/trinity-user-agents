package io.paritytech.polkadotapp.feature_chats_impl.domain.sessions

import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.feature_chats_api.domain.sessions.ContactChatSessionRefCounter
import io.paritytech.polkadotapp.feature_chats_api.domain.sessions.ContactChatSessionReference
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import timber.log.Timber
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class RealContactChatSessionRefCounter @Inject constructor() : ContactChatSessionRefCounter {
    private val mutex = Mutex()
    private val refCounts = mutableMapOf<AccountId, Int>()
    private val enabledIdsState = MutableStateFlow<Set<AccountId>>(emptySet())

    override val enabledIds: Flow<Set<AccountId>> = enabledIdsState

    override suspend fun requestSessionsEnabled(accountIds: Set<AccountId>, label: String): ContactChatSessionReference {
        return withContext(NonCancellable) {
            mutex.withLock {
                accountIds.forEach { accountId ->
                    val newCount = (refCounts[accountId] ?: 0) + 1
                    refCounts[accountId] = newCount
                    Timber.d("Requested contact session for $accountId by '$label', refCount=$newCount")
                }
                enabledIdsState.value = refCounts.keys.toSet()
            }
            RealContactChatSessionReference(accountIds, label)
        }
    }

    private inner class RealContactChatSessionReference(
        private val accountIds: Set<AccountId>,
        private val label: String
    ) : ContactChatSessionReference {
        private var released = false

        override suspend fun release() {
            withContext(NonCancellable) {
                mutex.withLock {
                    check(!released) { "Reference '$label' for accounts $accountIds has already been released" }
                    released = true

                    accountIds.forEach { accountId ->
                        val newCount = (refCounts[accountId] ?: 0) - 1
                        if (newCount <= 0) {
                            refCounts.remove(accountId)
                            Timber.d("Released contact session for $accountId by '$label', refCount=0")
                        } else {
                            refCounts[accountId] = newCount
                            Timber.d("Released contact session for $accountId by '$label', refCount=$newCount")
                        }
                    }
                    enabledIdsState.value = refCounts.keys.toSet()
                }
            }
        }
    }
}
