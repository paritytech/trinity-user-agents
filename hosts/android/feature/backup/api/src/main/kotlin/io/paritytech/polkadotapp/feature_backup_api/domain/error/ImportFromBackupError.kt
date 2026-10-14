package io.paritytech.polkadotapp.feature_backup_api.domain.error

import io.paritytech.polkadotapp.common.domain.errors.UserCancellation

sealed class ImportFromBackupError : Exception() {
    data object NotFound : ImportFromBackupError()
    data object Cancelled : ImportFromBackupError(), UserCancellation
    data object Corrupted : ImportFromBackupError()
    data class Unknown(val original: Throwable) : ImportFromBackupError()
}
