package io.paritytech.polkadotapp.feature_members_api.data.model

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

@Serializable
class RingStatus(
    val total: Int,
    val included: Int,
    @SerialName("immutable_since")
    val immutableSince: Long? = null
)

/**
 * [RingPosition.Included.ringPosition] counts within its page while [RingStatus.included] counts across the ring,
 * so the pages before the key's own have to be added back. [keysPerPage] is the ring keys page size, see
 * [io.paritytech.polkadotapp.feature_members_api.data.repository.MembersRepository.getRingKeysPageSize].
 */
fun RingStatus.includesKey(position: RingPosition, keysPerPage: Int): Boolean {
    val included = position.includedOrNull() ?: return false

    return this.included > included.ringPage * keysPerPage + included.ringPosition
}
