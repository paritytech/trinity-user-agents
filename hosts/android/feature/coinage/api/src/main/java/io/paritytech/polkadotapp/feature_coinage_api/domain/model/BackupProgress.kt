package io.paritytech.polkadotapp.feature_coinage_api.domain.model

sealed interface BackupProgress {
    data object NotStarted : BackupProgress

    sealed interface Initial : BackupProgress {
        data object Syncing : Initial
        data object Completed : Initial
    }

    sealed interface Deep : BackupProgress {
        data object Syncing : Deep
        data object Completed : Deep
    }

    data object Completed : BackupProgress

    data object Unknown : BackupProgress

    fun isInProgress() = this is Initial.Syncing || this is Deep.Syncing
}
