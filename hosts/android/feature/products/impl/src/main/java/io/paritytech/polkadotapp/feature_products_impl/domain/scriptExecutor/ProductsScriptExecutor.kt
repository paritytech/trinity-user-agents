package io.paritytech.polkadotapp.feature_products_impl.domain.scriptExecutor

import io.paritytech.polkadotapp.feature_products_impl.domain.bot.ProductsBotApi
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.ProductWorker
import kotlinx.coroutines.CoroutineScope

/**
 * Interface for executing Products scripts.
 *
 * The native side provides APIs via [ProductsBotApi] for scripts to send messages.
 * Once initialized it is the [ProductWorker] the chat surface drives.
 */
interface ProductsScriptExecutor : ProductWorker {
    /**
     * Initialize the bot script and set up the bridge for communication.
     * Must be called before any other methods.
     *
     * @param botApi Api for scripts to communicate back to native (send messages, etc.)
     * @param scope Coroutine scope that ties the executor's lifecycle. When cancelled, resources are disposed.
     */
    suspend fun initializeBot(botApi: ProductsBotApi, scope: CoroutineScope): Result<Unit>
}
