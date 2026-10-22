package io.paritytech.polkadotapp.feature_chats_impl.data.declarations

import kotlinx.serialization.Serializable

// The SCALE values of the Chat Extension v2 additions that the host keeps. They keep the field order and the
// integer widths of the wire types, so they encode to the wire bytes. A stored value starts with a version byte
// (02-DESIGN §20 item 1): a later shape gets a new version byte, and the reader keeps the old one.

/** The first stored shape. Chat Extension v2 kept only [calls] and [hopAttachments] of it. */
@Serializable
class ChatRoomFeaturesLocalV1(
    val preimageAttachments: Boolean,
    val search: Boolean,
    val calls: ChatRoomCallsLocal?,
    val readAloud: Boolean,
    val sealedAttachments: Boolean,
    val hopAttachments: Boolean,
)

@Serializable
class ChatRoomFeaturesLocal(
    val calls: ChatRoomCallsLocal?,
    val hopAttachments: Boolean,
)

@Serializable
class ChatRoomCallsLocal(val audio: Boolean, val video: Boolean)
