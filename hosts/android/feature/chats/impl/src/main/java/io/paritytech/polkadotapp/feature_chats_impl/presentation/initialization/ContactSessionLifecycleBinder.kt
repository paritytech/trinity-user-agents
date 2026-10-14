package io.paritytech.polkadotapp.feature_chats_impl.presentation.initialization

import io.paritytech.polkadotapp.common.data.app.AppLifecycleState
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.presentation.AppInitializer
import io.paritytech.polkadotapp.common.presentation.AppLifecycleObserver
import io.paritytech.polkadotapp.feature_chats_api.domain.sessions.ContactChatSessionRefCounter
import io.paritytech.polkadotapp.feature_chats_api.domain.sessions.ContactChatSessionReference
import io.paritytech.polkadotapp.feature_chats_api.domain.sessions.requestSessionEnabled
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ContactsRepository
import io.paritytech.polkadotapp.feature_chats_impl.domain.sessions.shouldStartPolling
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.onCompletion
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.withContext
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class ContactSessionLifecycleBinder @Inject constructor(
    private val refCounter: ContactChatSessionRefCounter,
    private val contactsRepository: ContactsRepository,
    private val appLifecycleObserver: AppLifecycleObserver,
) : AppInitializer {
    private val heldRefs = mutableMapOf<AccountId, ContactChatSessionReference>()

    context(scope: ComputationalScope)
    override fun initialize(): Result<Unit> = runCatching {
        combine(
            appLifecycleObserver.subscribe(),
            contactsRepository.subscribeContactsWithChatRequests(),
        ) { state, contacts ->
            if (state == AppLifecycleState.FOREGROUND) {
                contacts
                    .filter { it.shouldStartPolling() }
                    .map { it.contact.accountId }
                    .toSet()
            } else {
                emptySet()
            }
        }
            .onEach { target -> reconcile(target) }
            .onCompletion { releaseAll() }
            .launchIn(scope)
    }

    private suspend fun reconcile(target: Set<AccountId>) {
        withContext(NonCancellable) {
            val toRelease = heldRefs.keys - target
            toRelease.forEach { accountId ->
                heldRefs.remove(accountId)?.release()
            }
            val toAcquire = target - heldRefs.keys
            toAcquire.forEach { accountId ->
                heldRefs[accountId] = refCounter.requestSessionEnabled(accountId, "AppLifecycle")
            }
        }
    }

    private suspend fun releaseAll() {
        val refs = heldRefs.values.toList()
        heldRefs.clear()
        refs.forEach { it.release() }
    }
}
