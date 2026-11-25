package io.paritytech.polkadotapp.common.data.network

import com.neovisionaries.ws.client.WebSocketFactory
import io.novasama.substrate_sdk_android.wsrpc.SocketService

fun interface SocketServiceFactory {
    fun create(webSocketFactory: WebSocketFactory): SocketService
}
