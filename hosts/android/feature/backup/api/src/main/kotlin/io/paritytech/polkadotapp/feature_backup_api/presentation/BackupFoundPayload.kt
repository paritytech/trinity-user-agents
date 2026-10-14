package io.paritytech.polkadotapp.feature_backup_api.presentation

import android.os.Parcelable
import io.paritytech.polkadotapp.common.domain.model.Timestamp
import kotlinx.parcelize.Parcelize

@Parcelize
class BackupFoundPayload(
    val createdAt: Timestamp,
    val accountId: ByteArray
) : Parcelable {
    companion object {
        const val REQUEST_KEY = "c5a2f8e1-3d4b-4a9c-b6e7-8f1d2c3e4a5b"
    }

    enum class Result {
        RECOVERED,
        OVERRIDDEN
    }
}
