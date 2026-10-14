package io.paritytech.polkadotapp.tools_media_connection_api.domain

import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.StateFlow

typealias UseCaseId = String
typealias UseCaseData = ByteArray

interface DataTransport {
    val state: StateFlow<DataTransportState>

    fun subscribeMessages(id: UseCaseId): Flow<UseCaseData>

    suspend fun send(id: UseCaseId, data: UseCaseData)
    suspend fun awaitOpen()
    fun isOpen(): Boolean
}

enum class DataTransportState {
    Connecting, Open, Closing, Closed
}
