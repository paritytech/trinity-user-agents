package io.paritytech.polkadotapp.feature_products_impl.presentation

import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.ViewModelStore
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory

/** Creates the ViewModel in a store and clears the store, so `onCleared()` runs the way it does when a sheet goes away. */
inline fun <reified VM : ViewModel> createAndClear(crossinline create: () -> VM) {
    val store = ViewModelStore()
    ViewModelProvider.create(store, viewModelFactory { initializer { create() } })[VM::class]
    store.clear()
}
