package io.paritytech.polkadotapp.feature_chats_impl.presentation.dynamic

import io.paritytech.polkadotapp.feature_chats_api.domain.model.ChatMessageId
import io.paritytech.polkadotapp.feature_chats_api.presentation.dynamic.PerMessageStateProvider
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn

abstract class PerMessageStateHolder<S>(
    protected val scope: CoroutineScope,
    source: Flow<Map<ChatMessageId, S>>
) : PerMessageStateProvider<S> {
    private val states = source.stateIn(scope, SharingStarted.WhileSubscribed(), emptyMap())

    override fun observe(id: ChatMessageId): Flow<S?> {
        return states.map { it[id] }.distinctUntilChanged()
    }

    protected fun currentState(id: ChatMessageId): S? = states.value[id]
}
