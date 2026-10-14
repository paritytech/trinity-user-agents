package io.paritytech.polkadotapp.feature_chats_api.domain.extension

import io.paritytech.polkadotapp.common.utils.Identifiable

abstract class ExternalExtension : ChatExtension, Identifiable {
    override val identifier: String get() = id

    abstract suspend fun dispose()
}
