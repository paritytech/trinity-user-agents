package io.paritytech.polkadotapp.feature_chats_impl.data.declarations

import kotlinx.serialization.Serializable

// The SCALE values of the Chat Extension v2 additions that the host keeps. They keep the field order and the
// integer widths of the wire types, so they encode to the wire bytes. A stored value starts with a version byte
// (02-DESIGN §20 item 1): a later shape gets a new version byte, and the reader keeps the old one.

@Serializable
class ChatRoomFeaturesLocal(
    val preimageAttachments: Boolean,
    val search: Boolean,
    val calls: ChatRoomCallsLocal?,
    val threadOf: ChatThreadParentLocal?,
    val readAloud: Boolean,
)

@Serializable
class ChatRoomCallsLocal(val audio: Boolean, val video: Boolean)

@Serializable
class ChatThreadParentLocal(val roomId: String, val messageId: String)

@Serializable
class ChatMessageMetaValueLocal(
    val thread: ChatThreadSummaryLocal?,
    val branch: ChatBranchPositionLocal?,
    val reasoning: ChatReasoningLocal?,
)

/** [replyCount] holds the bits of a `u32`. */
@Serializable
class ChatThreadSummaryLocal(
    val roomId: String,
    val replyCount: Int,
    val lastReplyAt: Long?,
    val participantIds: List<String>,
)

/** [index] and [count] hold the bits of a `u16`. */
@Serializable
class ChatBranchPositionLocal(val index: Short, val count: Short)

/** [durationMs] holds the bits of a `u32`. */
@Serializable
class ChatReasoningLocal(val durationMs: Int?)
