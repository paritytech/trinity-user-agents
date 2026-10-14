package io.paritytech.polkadotapp.feature_members_api.data.model

import io.paritytech.polkadotapp.common.domain.model.DataByteArray
import kotlinx.serialization.Serializable

@Serializable
class RingRoot(
    val root: DataByteArray,
    val revision: RingRevision,
)
