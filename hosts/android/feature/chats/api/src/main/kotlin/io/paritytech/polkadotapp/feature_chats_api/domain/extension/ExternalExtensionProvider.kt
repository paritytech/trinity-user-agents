package io.paritytech.polkadotapp.feature_chats_api.domain.extension

import kotlinx.coroutines.flow.Flow

interface ExternalExtensionProvider {
    fun observeExtensions(): Flow<List<ExternalExtension>>
}
