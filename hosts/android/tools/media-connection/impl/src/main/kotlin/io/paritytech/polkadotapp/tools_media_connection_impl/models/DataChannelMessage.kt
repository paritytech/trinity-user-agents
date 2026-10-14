package io.paritytech.polkadotapp.tools_media_connection_impl.models

import io.paritytech.polkadotapp.tools_media_connection_api.domain.UseCaseData
import io.paritytech.polkadotapp.tools_media_connection_api.domain.UseCaseId
import kotlinx.serialization.Serializable

@Serializable
class DataChannelMessage(
    val id: UseCaseId,
    val data: UseCaseData
)
