package io.paritytech.polkadotapp.feature_products_impl.domain.worker

import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.update
import java.lang.ref.WeakReference

/**
 * A mutable, thread-safe holder for a modality-specific API bound onto a shared worker (e.g. chat
 * messaging). Set when a driver attaches, cleared when it detaches. The value is held weakly so a
 * live worker never keeps its driver alive, and read once per call so a concurrent rebind cannot
 * tear a partially updated pair.
 */
interface ModalityApiSlot<T : Any> {
    val bound: Flow<T?>

    fun set(value: T)

    /** Clears the slot only if [expected] is still the bound value. */
    fun clear(expected: T)
    fun tryUse(): Result<T>
}

class WeakModalityApiSlot<T : Any> : ModalityApiSlot<T> {
    private val ref = MutableStateFlow<WeakReference<T>?>(null)

    override val bound: Flow<T?> = ref.map { it?.get() }.distinctUntilChanged { old, new -> old === new }

    override fun set(value: T) {
        ref.value = WeakReference(value)
    }

    override fun clear(expected: T) {
        ref.update { current -> current?.takeUnless { it.get().let { held -> held == null || held === expected } } }
    }

    override fun tryUse(): Result<T> {
        val value = ref.value?.get() ?: return Result.failure(EmptyModalitySlot)
        return Result.success(value)
    }

    private object EmptyModalitySlot : Exception("No modality API is bound")
}
