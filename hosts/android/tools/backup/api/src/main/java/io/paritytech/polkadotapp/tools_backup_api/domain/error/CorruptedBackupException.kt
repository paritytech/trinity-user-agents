package io.paritytech.polkadotapp.tools_backup_api.domain.error

class CorruptedBackupException(cause: Throwable? = null) : IllegalStateException(cause)
