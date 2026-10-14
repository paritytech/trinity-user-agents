package io.paritytech.polkadotapp.feature_usernames_impl.presentation.claim

import io.paritytech.polkadotapp.common.presentation.ui.errors.NoConnectionPresentationError
import io.paritytech.polkadotapp.common.presentation.ui.errors.PresentationError
import io.paritytech.polkadotapp.common.presentation.ui.errors.ServerPresentationError
import io.paritytech.polkadotapp.common.presentation.ui.errors.StringResPresentationError
import io.paritytech.polkadotapp.feature_usernames_impl.domain.error.UsernameFlowError
import io.paritytech.polkadotapp.common.R as RCommon

fun UsernameFlowError.toPresentationError(): PresentationError = when (this) {
    UsernameFlowError.NoConnection -> NoConnectionPresentationError(this)

    UsernameFlowError.VerificationUnavailable ->
        StringResPresentationError(RCommon.string.username_error_verification_unavailable)

    UsernameFlowError.VerificationRejected ->
        StringResPresentationError(RCommon.string.username_error_verification_rejected)

    UsernameFlowError.VerificationBusy -> StringResPresentationError(RCommon.string.username_error_verification_busy)

    UsernameFlowError.Server -> ServerPresentationError(this)

    UsernameFlowError.Cancelled,
    UsernameFlowError.Unknown -> StringResPresentationError(RCommon.string.username_error_unknown)
}
