package io.paritytech.polkadotapp.tools_backup_api.domain.model

import io.paritytech.polkadotapp.common.domain.model.Timestamp

interface RestorableBackup {
    val createdAt: Timestamp

    suspend fun restore(): Result<Backup>
}
