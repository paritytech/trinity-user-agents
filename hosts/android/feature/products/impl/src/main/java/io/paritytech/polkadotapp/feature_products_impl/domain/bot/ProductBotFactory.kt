package io.paritytech.polkadotapp.feature_products_impl.domain.bot

import android.content.Context
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatRoomCallRepository
import io.paritytech.polkadotapp.feature_chats_api.domain.ChatRoomDeclarationsRepository
import io.paritytech.polkadotapp.feature_chats_api.domain.productRoom.ProductFeedListeners
import io.paritytech.polkadotapp.feature_products_api.model.Product
import io.paritytech.polkadotapp.feature_products_impl.domain.product.ProductImageResolver
import io.paritytech.polkadotapp.feature_products_impl.domain.truapi.ProductChatRoomEventSink
import io.paritytech.polkadotapp.feature_products_impl.domain.worker.ProductWorkerRefCounter
import javax.inject.Inject
import javax.inject.Singleton

/**
 * Factory for creating [ProductChatExtension] instances. The worker each extension drives is owned
 * by [ProductWorkerRefCounter], not built here.
 */
@Singleton
class ProductBotFactory @Inject constructor(
    @param:ApplicationContext private val appContext: Context,
    private val workerRefCounter: ProductWorkerRefCounter,
    private val imageResolver: ProductImageResolver,
    private val declarationsRepository: ChatRoomDeclarationsRepository,
    private val feedListeners: ProductFeedListeners,
    private val roomEventSink: ProductChatRoomEventSink,
    private val callRepository: ChatRoomCallRepository,
) {
    fun create(product: Product, origin: ProductChatExtensionOrigin): ProductChatExtension {
        return ProductChatExtension(
            appContext = appContext,
            product = product,
            workerRefCounter = workerRefCounter,
            imageResolver = imageResolver,
            declarationsRepository = declarationsRepository,
            feedListeners = feedListeners,
            roomEventSink = roomEventSink,
            callRepository = callRepository,
            origin = origin,
        )
    }
}
