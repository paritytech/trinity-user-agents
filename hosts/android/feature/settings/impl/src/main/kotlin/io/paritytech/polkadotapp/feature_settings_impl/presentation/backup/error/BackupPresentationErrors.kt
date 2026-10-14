package io.paritytech.polkadotapp.feature_settings_impl.presentation.backup.error

import io.paritytech.polkadotapp.common.presentation.ui.errors.PresentationError
import io.paritytech.polkadotapp.common.presentation.ui.errors.PresentationThrowable
import io.paritytech.polkadotapp.common.presentation.ui.errors.StringResPresentationError
import io.paritytech.polkadotapp.common.R as RCommon

class AuthenticationFailedPresentationError(cause: Throwable) :
    PresentationThrowable(cause),
    PresentationError by StringResPresentationError(RCommon.string.backup_error_authentication_failed)
