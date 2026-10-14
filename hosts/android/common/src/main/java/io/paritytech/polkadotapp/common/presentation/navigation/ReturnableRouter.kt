package io.paritytech.polkadotapp.common.presentation.navigation

interface ReturnableRouter {
    fun back()
    fun <T : Any> backWithResult(key: String, result: T)
}
