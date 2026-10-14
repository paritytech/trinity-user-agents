package io.paritytech.polkadotapp.common.presentation.screens

import androidx.annotation.StringRes
import androidx.lifecycle.SavedStateHandle
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import io.paritytech.polkadotapp.common.data.memory.ComputationalScope
import io.paritytech.polkadotapp.common.domain.errors.UserCancellation
import io.paritytech.polkadotapp.common.utils.OneShotEventChannel
import kotlinx.coroutines.flow.receiveAsFlow
import timber.log.Timber
import kotlin.coroutines.CoroutineContext
import io.paritytech.polkadotapp.common.presentation.ui.errors.PresentationError as PresentationErrorModel

open class BaseViewModel : ViewModel(), ComputationalScope, MessageDisplay {
    private val _events = OneShotEventChannel<BaseViewModelEvent>()
    val events = _events.receiveAsFlow()

    override fun showMessage(text: String) {
        _events.trySend(BaseViewModelEvent.Message(text))
    }

    protected fun showMessage(
        @StringRes messageRes: Int,
    ) {
        _events.trySend(BaseViewModelEvent.ResourceMessage(messageRes))
    }

    // The single seam for a caught failure: user cancellations never reach the user and everything else is always logged.
    protected fun <T> showPresentationError(error: T) where T : Throwable, T : PresentationErrorModel {
        if (shouldIgnore(error)) return

        Timber.e(error)

        _events.trySend(BaseViewModelEvent.PresentationError(error))
    }

    @Deprecated("Use showPresentationError instead")
    protected fun showError(text: String) {
        _events.trySend(BaseViewModelEvent.Error(text))
    }

    @Deprecated("Use showPresentationError instead")
    protected fun showError(throwable: Throwable) {
        if (!shouldIgnore(throwable)) {
            Timber.e(throwable)

            showError(throwable.message ?: "Unknown failure")
        }
    }

    override val coroutineContext: CoroutineContext
        get() = viewModelScope.coroutineContext

    // A presentation error carries the cancellation marker on its cause rather than on itself.
    private fun shouldIgnore(throwable: Throwable): Boolean {
        return generateSequence(throwable, Throwable::cause).any { it is UserCancellation }
    }

    protected inline fun <reified T> SavedStateHandle.getPayload(
        key: String = T::class.java.name,
    ): T {
        return get(key)!!
    }
}

sealed class BaseViewModelEvent {
    class Error(val errorTitle: String) : BaseViewModelEvent()

    class Message(val message: String) : BaseViewModelEvent()

    class ResourceMessage(@param:StringRes val messageRes: Int) : BaseViewModelEvent()

    class PresentationError(val error: PresentationErrorModel) : BaseViewModelEvent()
}
