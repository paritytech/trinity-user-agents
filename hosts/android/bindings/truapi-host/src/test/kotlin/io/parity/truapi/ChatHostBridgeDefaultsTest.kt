package io.parity.truapi

import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import uniffi.truapi.ChatBotRegistrationStatus
import uniffi.truapi.ChatFeedUpdate
import uniffi.truapi.ChatMessageContent
import uniffi.truapi.ChatMessageStatus
import uniffi.truapi.ChatRoom
import uniffi.truapi.ChatRoomChangeOutcome
import uniffi.truapi.ChatRoomConfig
import uniffi.truapi.ChatRoomFooter
import uniffi.truapi.ChatRoomMetadata
import uniffi.truapi.ChatRoomParticipation
import uniffi.truapi.ChatRoomPreview
import uniffi.truapi.ChatRoomRegistrationStatus
import uniffi.truapi.ChatRoomState
import uniffi.truapi.HostChatCreateRoomException
import uniffi.truapi.HostRejection

// The iOS package tests the same defaults, so both hosts serve a host that has
// not implemented the Chat Extension v2 requests alike.
class ChatHostBridgeDefaultsTest {
    // A product may declare the configuration that a host without the v2
    // requests serves anyway, so that host must accept it, and the core then
    // finds the room in the room list of that host.
    @Test
    fun `the defaults accept the configuration of a new room`() = runBlocking<Unit> {
        val bridge = FirstVersionChatHostBridge()

        val status = bridge.createRoom("room", "Room", "", HOST_DEFAULT_CHAT_ROOM_CONFIG)

        assertEquals(ChatRoomRegistrationStatus.NEW, status)
        assertEquals(listOf("room"), bridge.createdRooms)
        val hosted = ChatRoomState(ChatRoomParticipation.ROOM_HOST, HOST_DEFAULT_CHAT_ROOM_CONFIG)
        assertEquals(hosted, bridge.room("room"))
        assertNull(bridge.room("other"))
    }

    // A declaration that the host cannot serve must fail, not disappear. A
    // configuration it cannot keep fails before the room exists, so no room is
    // left with the behaviour that the product did not ask for. A change it
    // does not serve answers `UNSUPPORTED`, so the product can tell it from a
    // failure worth a retry.
    @Test
    fun `the defaults refuse what the host does not serve`() = runBlocking<Unit> {
        val bridge = FirstVersionChatHostBridge()
        val sourced = ChatRoomConfig(productSource = true, reactionDetails = false)

        val refusal = runCatching { bridge.createRoom("room", "Room", "", sourced) }.exceptionOrNull()

        assertTrue("$refusal", refusal is HostChatCreateRoomException.UnsupportedConfig)
        assertEquals(emptyList<String>(), bridge.createdRooms)
        val metadata = ChatRoomMetadata(name = "Room", description = null, icon = null, placeholder = null)
        val preview = ChatRoomPreview(
            timestamp = null,
            badge = null,
            lastMessage = null,
            lastReaction = null,
            unseenReactions = 0u,
            pinned = false,
            activity = null,
        )
        val update = ChatFeedUpdate.StatusChanged(messageId = "message", status = ChatMessageStatus.READ)
        assertEquals(
            List(4) { ChatRoomChangeOutcome.UNSUPPORTED },
            listOf(
                bridge.setRoomMetadata("room", metadata),
                bridge.setRoomPreview("room", preview),
                bridge.deleteRoom("room"),
                bridge.updateFeed("room", update, persist = true),
            ),
        )
    }

    // The core reaches a bridge as a `ChatHostBridge`. A host that keeps
    // configurations overrides the four-argument `createRoom` and `room`, and
    // the core must reach those overrides, not the defaults.
    @Test
    fun `a host that keeps configurations serves them through the interface`() = runBlocking<Unit> {
        val declaring = DeclaringChatHostBridge()
        val bridge: ChatHostBridge = declaring
        val sourced = ChatRoomConfig(productSource = true, reactionDetails = false)

        val status = bridge.createRoom("room", "Room", "", sourced)

        assertEquals(ChatRoomRegistrationStatus.NEW, status)
        assertEquals(mapOf("room" to sourced), declaring.configs)
        assertEquals(ChatRoomState(ChatRoomParticipation.ROOM_HOST, sourced), bridge.room("room"))
    }

    // A host that keeps configurations implements only the four-argument
    // `createRoom`, and a host without rooms implements neither. Each default
    // `createRoom` must not call the other for ever: the core reaches the
    // four-argument one, and a host that implements neither refuses the room.
    @Test
    fun `a host implements only the createRoom it needs`() = runBlocking<Unit> {
        val declaring = DeclaringChatHostBridge()
        val bridge: ChatHostBridge = declaring

        val status = bridge.createRoom("room", "Room", "", HOST_DEFAULT_CHAT_ROOM_CONFIG)

        assertEquals(ChatRoomRegistrationStatus.NEW, status)
        assertEquals(mapOf("room" to HOST_DEFAULT_CHAT_ROOM_CONFIG), declaring.configs)
        val roomless: ChatHostBridge = RoomlessChatHostBridge()
        val refusal = runCatching {
            roomless.createRoom("room", "Room", "", HOST_DEFAULT_CHAT_ROOM_CONFIG)
        }.exceptionOrNull()
        assertTrue("$refusal", refusal is HostRejection)
    }
}

// A host that keeps the configuration of each room, and so implements only the
// four-argument `createRoom`.
private class DeclaringChatHostBridge : ChatHostBridge {
    val configs = mutableMapOf<String, ChatRoomConfig>()

    override suspend fun createRoom(
        roomId: String,
        name: String,
        icon: String,
        config: ChatRoomConfig,
    ): ChatRoomRegistrationStatus {
        configs[roomId] = config
        return ChatRoomRegistrationStatus.NEW
    }

    override suspend fun room(roomId: String): ChatRoomState? =
        configs[roomId]?.let { ChatRoomState(ChatRoomParticipation.ROOM_HOST, it) }

    override suspend fun registerBot(botId: String, name: String, icon: String) = ChatBotRegistrationStatus.NEW

    override suspend fun postMessage(roomId: String, content: ChatMessageContent, alt: String?) = "message-id"

    override suspend fun setRoomFooter(roomId: String, footer: ChatRoomFooter) = Unit

    override suspend fun listRooms(): List<ChatRoom> = emptyList()
}

// A host that implements only the requests of the first chat version.
private class FirstVersionChatHostBridge : ChatHostBridge {
    val createdRooms = mutableListOf<String>()

    override suspend fun createRoom(roomId: String, name: String, icon: String): ChatRoomRegistrationStatus {
        createdRooms += roomId
        return ChatRoomRegistrationStatus.NEW
    }

    override suspend fun registerBot(botId: String, name: String, icon: String) = ChatBotRegistrationStatus.NEW

    override suspend fun postMessage(roomId: String, content: ChatMessageContent, alt: String?) = "message-id"

    override suspend fun setRoomFooter(roomId: String, footer: ChatRoomFooter) = Unit

    override suspend fun listRooms(): List<ChatRoom> =
        createdRooms.map { ChatRoom(it, ChatRoomParticipation.ROOM_HOST) }
}

// A host that creates no rooms: it implements neither `createRoom`.
private class RoomlessChatHostBridge : ChatHostBridge {
    override suspend fun registerBot(botId: String, name: String, icon: String) = ChatBotRegistrationStatus.NEW

    override suspend fun postMessage(roomId: String, content: ChatMessageContent, alt: String?) = "message-id"

    override suspend fun setRoomFooter(roomId: String, footer: ChatRoomFooter) = Unit

    override suspend fun listRooms(): List<ChatRoom> = emptyList()
}
