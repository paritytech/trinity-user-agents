package io.paritytech.polkadotapp.feature_chats_impl.presentation.initialization

import io.paritytech.polkadotapp.common.data.app.AppLifecycleState
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.data.time.TimeProvider
import io.paritytech.polkadotapp.common.presentation.AppInitializer
import io.paritytech.polkadotapp.common.presentation.AppLifecycleObserver
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.feature_chats_impl.data.repository.ContactsRepository
import io.paritytech.polkadotapp.feature_chats_impl.domain.usecase.SyncContactProfileUseCase
import io.paritytech.polkadotapp.feature_chats_impl.domain.usecase.isProfileCheckDue
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.conflate
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.onEach
import javax.inject.Inject
import javax.inject.Singleton

/**
 * While the app is foregrounded, every contact whose profile check is due gets one, in order. A
 * check that lands rewrites the contact row, which re-emits the list with that contact no longer
 * due; a check that fails leaves the row alone, so nothing re-emits and nothing loops.
 */
@Singleton
class ContactProfileLifecycleBinder @Inject constructor(
    private val contactsRepository: ContactsRepository,
    private val appLifecycleObserver: AppLifecycleObserver,
    private val syncContactProfileUseCase: SyncContactProfileUseCase,
    private val timeProvider: TimeProvider,
) : AppInitializer {
    context(scope: ComputationalScope)
    override fun initialize(): Result<Unit> = runCatching {
        combine(
            appLifecycleObserver.subscribe(),
            contactsRepository.subscribeContacts(),
        ) { state, contacts ->
            if (state == AppLifecycleState.FOREGROUND) {
                val now = timeProvider.now()
                contacts.filter { it.isProfileCheckDue(now) }
            } else {
                emptyList()
            }
        }
            .conflate()
            .onEach { due ->
                due.forEach { contact ->
                    syncContactProfileUseCase.sync(contact, force = false).logFailure("ContactProfile sync ${contact.username}")
                }
            }
            .launchIn(scope)
    }
}
