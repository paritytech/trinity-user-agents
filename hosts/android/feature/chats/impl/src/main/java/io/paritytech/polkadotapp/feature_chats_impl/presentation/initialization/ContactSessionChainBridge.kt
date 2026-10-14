package io.paritytech.polkadotapp.feature_chats_impl.presentation.initialization

import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.chains.multiNetwork.connection.ChainConnectionRefCounter
import io.paritytech.polkadotapp.chains.multiNetwork.connection.EnabledChainConnectionReference
import io.paritytech.polkadotapp.chains.multiNetwork.connection.requestConnectionEnabled
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.presentation.AppInitializer
import io.paritytech.polkadotapp.feature_chats_api.domain.sessions.ContactChatSessionRefCounter
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.onCompletion
import kotlinx.coroutines.flow.onEach
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class ContactSessionChainBridge @Inject constructor(
    private val contactChatSessionRefCounter: ContactChatSessionRefCounter,
    private val chainConnectionRefCounter: ChainConnectionRefCounter,
    private val knownChains: KnownChains,
) : AppInitializer {
    context(scope: ComputationalScope)
    override fun initialize(): Result<Unit> = runCatching {
        var heldRef: EnabledChainConnectionReference? = null
        contactChatSessionRefCounter.enabledIds
            .map { it.isNotEmpty() }
            .distinctUntilChanged()
            .onEach { shouldHold ->
                val newRef = if (shouldHold) {
                    chainConnectionRefCounter.requestConnectionEnabled(knownChains.people, "AnyContactSession")
                } else {
                    null
                }
                heldRef?.release()
                heldRef = newRef
            }
            .onCompletion {
                heldRef?.release()
                heldRef = null
            }
            .launchIn(scope)
    }
}
