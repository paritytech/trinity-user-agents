package io.paritytech.polkadotapp.feature_chats_impl.domain.usecase

import io.novasama.substrate_sdk_android.extensions.toHexString
import io.paritytech.polkadotapp.chains.multiNetwork.ChainRegistry
import io.paritytech.polkadotapp.chains.multiNetwork.KnownChains
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.utils.flatMap
import io.paritytech.polkadotapp.common.utils.mapError
import io.paritytech.polkadotapp.feature_chain_resources_api.data.repository.ResourcesRepository
import io.paritytech.polkadotapp.feature_chats_api.domain.PersonChatContacts
import io.paritytech.polkadotapp.feature_chats_api.domain.PersonChatRooms
import io.paritytech.polkadotapp.feature_chats_api.domain.error.StartChatError
import io.paritytech.polkadotapp.feature_chats_api.domain.error.asStartChatError
import io.paritytech.polkadotapp.feature_chats_impl.domain.middleware.ChatExtensionRegistry
import io.paritytech.polkadotapp.feature_chats_impl.domain.models.StartChatData
import javax.inject.Inject

class StartChatDataUseCase @Inject constructor(
    knownChains: KnownChains,
    private val chainRegistry: ChainRegistry,
    private val resourcesRepository: ResourcesRepository,
    private val personChatRooms: PersonChatRooms,
    private val chatExtensionRegistry: ChatExtensionRegistry,
    private val personChatContacts: PersonChatContacts,
) {
    private val chainId = knownChains.assetHub

    // The chat product gives the room of the chat with the account. For an account that is no contact yet, the room
    // shows the invite composer, and the first message of the user is the welcome message of the request.
    suspend operator fun invoke(contactAccountId: AccountId): Result<StartChatData> {
        // An account that is no contact yet must have a chat key on chain, as the native chat checks.
        if (personChatContacts.getContacts().none { it.accountId == contactAccountId }) {
            val registered = resourcesRepository.consumerInfo(chainRegistry.getChain(chainId).id, contactAccountId)
                .mapError(Throwable::asStartChatError)
                .getOrElse { return Result.failure(it) }
            if (registered == null) return Result.failure(StartChatError.PeerNotRegistered)
        }

        // Before the network is known the chat product cannot open a room yet; the account is not at fault.
        val anchor = personChatRooms.homeRoom()
            ?: return Result.failure(StartChatError.Unknown(IllegalStateException("The network of the chat product is not known yet")))
        val querying = chatExtensionRegistry.getExtensionForChat(anchor)?.roomQuerying(anchor)
            ?: return Result.failure<StartChatData>(IllegalStateException("The chat product does not run")).mapError(Throwable::asStartChatError)
        return querying.openLink(anchor, "contact:${contactAccountId.value.toHexString()}")
            .flatMap { room -> room?.let { Result.success(StartChatData.ProductRoom(it)) } ?: Result.failure(IllegalStateException("The chat product gave no room")) }
            .mapError(Throwable::asStartChatError)
    }
}
