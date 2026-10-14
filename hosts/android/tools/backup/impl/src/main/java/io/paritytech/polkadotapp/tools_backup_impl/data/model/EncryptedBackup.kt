package io.paritytech.polkadotapp.tools_backup_impl.data.model

import io.paritytech.polkadotapp.common.domain.model.Timestamp

@JvmInline
value class EncryptedBackup(val value: ByteArray)

class RestoredEncryptedBackup(
    val backup: EncryptedBackup,
    val createdAt: Timestamp
)
