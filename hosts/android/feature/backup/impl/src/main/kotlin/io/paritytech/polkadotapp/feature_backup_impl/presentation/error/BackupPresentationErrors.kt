package io.paritytech.polkadotapp.feature_backup_impl.presentation.error

import io.paritytech.polkadotapp.common.presentation.ui.errors.PresentationError
import io.paritytech.polkadotapp.common.presentation.ui.errors.PresentationThrowable
import io.paritytech.polkadotapp.common.presentation.ui.errors.StringResPresentationError
import io.paritytech.polkadotapp.common.presentation.ui.errors.UnexpectedPresentationError
import io.paritytech.polkadotapp.feature_backup_api.domain.error.ImportFromBackupError
import io.paritytech.polkadotapp.common.R as RCommon

class WrongMnemonicPresentationError :
    PresentationThrowable(null),
    PresentationError by StringResPresentationError(RCommon.string.backup_error_wrong_mnemonic)

class BackupNotFoundPresentationError(cause: Throwable) :
    PresentationThrowable(cause),
    PresentationError by StringResPresentationError(RCommon.string.backup_not_found_error)

class BackupCorruptedPresentationError(cause: Throwable) :
    PresentationThrowable(cause),
    PresentationError by StringResPresentationError(RCommon.string.backup_corrupted_error)

// Cancelled is a UserCancellation, so BaseViewModel drops it before the message is ever resolved.
fun Throwable.toImportFromBackupPresentationError(): PresentationThrowable = when (this) {
    ImportFromBackupError.NotFound -> BackupNotFoundPresentationError(this)
    ImportFromBackupError.Corrupted -> BackupCorruptedPresentationError(this)
    else -> UnexpectedPresentationError(this)
}
