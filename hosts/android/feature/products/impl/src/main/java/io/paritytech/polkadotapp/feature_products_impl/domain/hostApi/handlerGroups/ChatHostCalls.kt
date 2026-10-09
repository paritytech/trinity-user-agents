package io.paritytech.polkadotapp.feature_products_impl.domain.hostApi.handlerGroups

import io.paritytech.polkadotapp.common.domain.model.hexToDataByteArray
import io.paritytech.polkadotapp.common.utils.HexString
import io.paritytech.polkadotapp.feature_products_api.model.ProductChatIdParameter
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.ProductBotMessage
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.ProductsBotApi
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.model.CreateProductRoomRequest
import io.paritytech.polkadotapp.feature_products_impl.domain.bot.model.ProductChatRoom
import io.paritytech.polkadotapp.feature_products_impl.domain.jsEngine.ContainerBridge

class ChatHostCalls(
    private val botApi: ProductsBotApi,
) : HostCallHandlerGroup {
    override fun registerOn(bridge: ContainerBridge) {
        bridge.registerHandler<ChatCreateRoomParams, ChatCreateRoomResult>("chatCreateRoom") { params ->
            val request = CreateProductRoomRequest(
                chatIdParameter = ProductChatIdParameter(params.roomId),
                name = params.name,
                icon = params.icon,
            )
            botApi.createRoom(request).map { ChatCreateRoomResult(it.status.name) }
        }

        bridge.registerHandler<ChatSendTextParams, ChatSendResult>("chatSendTextMessage") { params ->
            botApi.sendMessage(ProductChatIdParameter(params.chatId), ProductBotMessage.Text(params.text))
                .map(::ChatSendResult)
        }

        bridge.registerHandler<ChatSendCustomParams, ChatSendResult>("chatSendCustomMessage") { params ->
            val data = params.payloadHex.hexToDataByteArray()
            botApi.sendMessage(ProductChatIdParameter(params.chatId), ProductBotMessage.Custom(params.messageType, data))
                .map(::ChatSendResult)
        }

        bridge.registerSubscription<Unit, List<ProductChatRoom>>("chatListSubscribe") {
            botApi.subscribeChatRooms()
        }
    }
}

private data class ChatCreateRoomParams(val roomId: String, val name: String, val icon: String?)
private data class ChatCreateRoomResult(val status: String)
private data class ChatSendTextParams(val text: String, val chatId: String)
private data class ChatSendCustomParams(val messageType: String, val payloadHex: HexString, val chatId: String)
private data class ChatSendResult(val messageId: String)
