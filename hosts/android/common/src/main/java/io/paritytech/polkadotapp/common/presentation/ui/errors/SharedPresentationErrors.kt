package io.paritytech.polkadotapp.common.presentation.ui.errors

import io.paritytech.polkadotapp.common.R

class NoConnectionPresentationError(cause: Throwable) :
    PresentationThrowable(cause),
    PresentationError by StringResPresentationError(R.string.common_error_no_connection)

class ServerPresentationError(cause: Throwable) :
    PresentationThrowable(cause),
    PresentationError by StringResPresentationError(R.string.common_error_server)

class SigningFailedPresentationError(cause: Throwable) :
    PresentationThrowable(cause),
    PresentationError by StringResPresentationError(R.string.common_error_signing_failed)

class UnexpectedPresentationError(cause: Throwable) :
    PresentationThrowable(cause),
    PresentationError by StringResPresentationError(R.string.generic_error_notification)
