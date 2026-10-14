package io.paritytech.polkadotapp.feature_backup_api.presentation

import android.os.Parcelable
import io.paritytech.polkadotapp.common.domain.model.Timestamp
import kotlinx.parcelize.Parcelize

@Parcelize
data class BackupConflictPayload(
    val backupCreatedAt: Timestamp
) : Parcelable
