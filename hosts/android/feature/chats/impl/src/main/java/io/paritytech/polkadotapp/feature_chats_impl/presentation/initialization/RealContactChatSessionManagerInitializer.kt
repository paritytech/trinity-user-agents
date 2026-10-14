package io.paritytech.polkadotapp.feature_chats_impl.presentation.initialization

import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.presentation.AppInitializer
import io.paritytech.polkadotapp.feature_chats_impl.domain.sessions.RealContactChatSessionManager
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
internal class RealContactChatSessionManagerInitializer @Inject constructor(
    private val manager: RealContactChatSessionManager,
) : AppInitializer {
    context(scope: ComputationalScope)
    override fun initialize(): Result<Unit> = runCatching {
        manager.startSubscriptions()
    }
}
