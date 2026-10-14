package io.paritytech.polkadotapp.common.presentation.ui.errors

import androidx.annotation.StringRes
import androidx.compose.runtime.Composable
import androidx.compose.ui.res.stringResource

interface PresentationError {
    @Composable
    fun message(): String
}

// Kotlin has no intersection types, so anything returning "a Throwable that is also a PresentationError" names this instead.
abstract class PresentationThrowable(cause: Throwable?) : Throwable(cause), PresentationError

// To be used via delegation: class XError(cause: Throwable) : PresentationThrowable(cause), PresentationError by StringResPresentationError(R.string.x_error)
class StringResPresentationError(
    @param:StringRes private val messageRes: Int
) : PresentationError {
    @Composable
    override fun message(): String {
        return stringResource(messageRes)
    }
}
