package io.paritytech.polkadotapp.database.model

import androidx.room.Entity

@Entity(tableName = "sso_handled_requests", primaryKeys = ["sessionId", "requestId"])
class SsoHandledRequestLocal(
    val sessionId: String,
    val requestId: String
)
