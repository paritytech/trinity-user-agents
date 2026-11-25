import Coinage

/// Durability group ids for a chat transfer. The claim group stays the raw message id.
enum ChatTransferGroups {
    /// The reclaim of a payment's unclaimed coins, kept apart from the message's own claim group.
    static func reclaim(_ messageId: Chat.MessageId) -> CoinageTxGroupId {
        "chat-reclaim:\(messageId)"
    }
}
