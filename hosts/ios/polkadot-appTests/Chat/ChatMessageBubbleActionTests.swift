import Testing
@testable import polkadot_app

/// The menu of a message offers Delete for an own message in a room whose messages the product keeps.
struct ChatMessageBubbleActionTests {
    @Test(arguments: [
        (Chat.LocalMessage.Status.outgoing(.sent), true, true),
        (.outgoing(.sent), false, false),
        (.incoming(.seen), true, false)
    ])
    func deleteIsOfferedForAnOwnMessageOfAProductSourceRoom(
        status: Chat.LocalMessage.Status,
        isProductSource: Bool,
        offersDelete: Bool
    ) {
        let actions = ChatMessageBubbleAction.textMessageActions(
            for: status,
            input: .empty,
            isEdited: false,
            isProductSource: isProductSource
        )

        #expect(actions.contains(.delete) == offersDelete)
    }
}
