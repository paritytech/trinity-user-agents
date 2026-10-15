package io.paritytech.polkadotapp.feature_products_impl.domain.truapi.worker

import io.parity.truapi.TrUAPIProductExecution
import io.paritytech.polkadotapp.feature_products_api.model.ProductId
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.ProductChatMessaging
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.model.ProductChatRoom
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.toCoreChatRoom
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.launchIn
import kotlinx.coroutines.flow.onEach
import kotlinx.coroutines.flow.retryWhen
import timber.log.Timber

class TrUAPIChatRoomForwarding(
    private val productId: ProductId,
    private val execution: TrUAPIProductExecution,
    private val chatMessaging: ProductChatMessaging,
) {
    fun start(scope: CoroutineScope): Job = chatMessaging.subscribeChatRooms()
        .onEach { rooms -> notify(rooms) }
        .retryWhen { failure, attempt ->
            if (failure is CancellationException) return@retryWhen false
            Timber.w(failure, "TrUAPI chat room forwarding for %s failed; resubscribing", productId.value)
            delay(reopenBackoff(attempt))
            true
        }
        .launchIn(scope)

    // A bad room or a dead execution must not reach retry, which would resubscribe forever.
    @Suppress("TooGenericExceptionCaught")
    private fun notify(rooms: List<ProductChatRoom>) {
        try {
            execution.notifyChatRoomsChanged(rooms.map { it.toCoreChatRoom() })
        } catch (error: CancellationException) {
            throw error
        } catch (error: Exception) {
            Timber.w(error, "TrUAPI chat room forwarding for %s dropped a room list", productId.value)
        }
    }
}
