package io.paritytech.polkadotapp.feature_products_impl.domain.derivation

import io.paritytech.polkadotapp.feature_chats_api.domain.derivation.ChatProductSubtreeProvider
import io.paritytech.polkadotapp.feature_dotns_api.domain.DotNsTldProvider
import io.paritytech.polkadotapp.feature_dotns_api.domain.getTldRetrying
import io.paritytech.polkadotapp.feature_products_api.model.derivation.ReservedProductIds
import io.paritytech.polkadotapp.feature_products_api.model.derivation.productSubtreePath

class ReservedChatProductSubtreeProvider(
    private val dotNsTldProvider: DotNsTldProvider,
) : ChatProductSubtreeProvider {
    override suspend fun subtreePath(): String {
        val tld = dotNsTldProvider.getTldRetrying()
        return productSubtreePath(ReservedProductIds.chat(tld))
    }
}
