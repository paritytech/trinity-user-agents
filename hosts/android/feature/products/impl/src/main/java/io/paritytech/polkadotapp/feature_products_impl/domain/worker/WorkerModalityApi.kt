package io.paritytech.polkadotapp.feature_products_impl.domain.worker

import io.paritytech.polkadotapp.feature_products_impl.domain.bot.ProductChatMessaging

/**
 * A modality-specific API a consumer binds onto a shared worker while it drives it. Today only chat
 * drives; other modalities (e.g. a full-page driver) become new variants.
 */
sealed interface WorkerModalityApi {
    class Chat(val messaging: ProductChatMessaging) : WorkerModalityApi
}
