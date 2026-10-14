package io.paritytech.polkadotapp.database.model

import androidx.room.Embedded
import androidx.room.Relation

class SsoSessionWithMetadata(
    @Embedded
    val session: SsoSessionLocal,
    @Relation(
        parentColumn = "sharedSecretPublicKey",
        entityColumn = "sessionSharedSecretPublicKey",
        entity = SsoSessionMetadataLocal::class,
    )
    val metadata: List<SsoSessionMetadataLocal>,
)
