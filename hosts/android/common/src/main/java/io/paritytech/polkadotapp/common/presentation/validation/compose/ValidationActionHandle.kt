package io.paritytech.polkadotapp.common.presentation.validation.compose

import androidx.compose.runtime.Composable
import androidx.compose.runtime.MutableState
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import io.paritytech.polkadotapp.common.domain.validation.ValidationUserInputAction
import io.paritytech.polkadotapp.common.presentation.validation.ValidationMixin
import io.paritytech.polkadotapp.common.utils.AwaitableAction
import io.paritytech.polkadotapp.design.utils.collectAsEffect

/**
 * Compose handle for the latest unresolved [ValidationUserInputAction] of type [I] emitted by
 * [ValidationMixin.userInputAction].
 *
 * Obtain via [rememberValidationActionHandle]. Read [payload]/[isVisible] from the composition
 * to drive UI; call [respond] to forward the user's decision back to the validation pipeline.
 * [respond] is a no-op if no action is pending, so it is safe to wire to dialog dismiss callbacks.
 */
class ValidationActionHandle<I : ValidationUserInputAction<O>, O> @PublishedApi internal constructor(
    private val state: MutableState<AwaitableAction<I, O>?>,
) {
    val payload: I? get() = state.value?.payload

    val isVisible: Boolean get() = state.value != null

    fun respond(result: O) {
        val current = state.value ?: return
        state.value = null
        current.onSuccess(result)
    }
}

/**
 * Subscribes to [ValidationMixin.userInputAction] and captures the latest unresolved action of
 * type [I]. Actions whose payload is not [I] are silently discarded by this handle.
 *
 * **Single-handle-per-mixin constraint.** The underlying [ValidationMixin.userInputAction] is
 * backed by a single-consumer channel (`Channel.receiveAsFlow()`). Each emission is delivered to
 * exactly one collector. If two `rememberValidationActionHandle` calls subscribe to the same
 * mixin, they will race for events and one will silently swallow actions meant for the other.
 *
 * Therefore: **one handle per mixin per screen.** If a screen drives several
 * [ValidationUserInputAction] subtypes through the same mixin, parameterize the handle with the
 * shared `ValidationUserInputAction<*>` supertype (or a sealed base) and `when`-dispatch on
 * [payload] inside the screen. Don't spin up a second handle for a sibling subtype.
 */
@Composable
inline fun <reified I : ValidationUserInputAction<O>, O> ValidationMixin.rememberValidationActionHandle(): ValidationActionHandle<I, O> {
    val state = remember { mutableStateOf<AwaitableAction<I, O>?>(null) }

    userInputAction.collectAsEffect { _, action ->
        if (action.payload is I) {
            @Suppress("UNCHECKED_CAST")
            state.value = action as AwaitableAction<I, O>
        }
    }

    return remember(state) { ValidationActionHandle(state) }
}
