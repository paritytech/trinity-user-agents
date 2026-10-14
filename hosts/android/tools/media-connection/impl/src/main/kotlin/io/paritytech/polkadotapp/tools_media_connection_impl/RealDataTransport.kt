package io.paritytech.polkadotapp.tools_media_connection_impl

import io.paritytech.polkadotapp.tools_media_connection_api.domain.DataTransport
import io.paritytech.polkadotapp.tools_media_connection_api.domain.DataTransportState
import io.paritytech.polkadotapp.tools_media_connection_api.domain.UseCaseData
import io.paritytech.polkadotapp.tools_media_connection_api.domain.UseCaseId
import io.paritytech.polkadotapp.tools_media_connection_impl.signaling.DataChannelMessaging
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.StateFlow

class RealDataTransport(
    private val dataChannelSignaling: DataChannelMessaging
) : DataTransport {
    override val state: StateFlow<DataTransportState> = dataChannelSignaling.state

    override suspend fun send(id: UseCaseId, data: UseCaseData) {
        dataChannelSignaling.sendMessage(id, data)
    }

    override fun subscribeMessages(id: UseCaseId): Flow<UseCaseData> {
        return dataChannelSignaling.subscribeMessages(id)
    }

    override suspend fun awaitOpen() {
        dataChannelSignaling.awaitChannelOpen()
    }

    override fun isOpen(): Boolean = dataChannelSignaling.isOpen()
}
