package io.paritytech.polkadotapp.feature_device_sync_impl.domain.engine

import io.paritytech.polkadotapp.feature_chats_api.domain.interactors.AddContactUseCase
import io.paritytech.polkadotapp.feature_chats_api.domain.interactors.ApplyRemoteChatMessageUseCase
import io.paritytech.polkadotapp.feature_sso_api.domain.GetActiveSsoSessionsUseCase
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.merge
import javax.inject.Inject

class LocalSyncTrigger @Inject constructor(
    private val getActiveSsoSessionsUseCase: GetActiveSsoSessionsUseCase,
    private val addContactUseCase: AddContactUseCase,
    private val applyRemoteChatMessageUseCase: ApplyRemoteChatMessageUseCase,
) {
    fun observe(): Flow<Unit> = merge(
        addContactUseCase.observeContactsChanged(),
        applyRemoteChatMessageUseCase.observeLocalMessageChanges(),
        getActiveSsoSessionsUseCase.observeSessionsChanged(),
    )
}
