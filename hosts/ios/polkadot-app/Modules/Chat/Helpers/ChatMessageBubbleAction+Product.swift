import Foundation

extension ChatMessageBubbleAction {
    /// The built-in entry that the product can hide, or nil for an entry that is not a built-in.
    var builtIn: ChatRoomPolicy.BuiltInMessageAction? {
        switch self {
        case .reply: .reply
        case .edit: .edit
        case .copy: .copy
        case .reaction: .react
        case .retry: .retry
        case .delete: .delete
        case .edited,
             .product: nil
        }
    }

    /// The entries of a message menu after the answer of `message_actions_query` (02-DESIGN §6.6).
    ///
    /// `Custom` gives only the entries of the product. `ExtendBuiltIn` gives the host entries without `hidden`, then
    /// the entries of the product. A destructive product entry goes last. No answer gives the host entries.
    static func productMenu(
        hostActions: [ChatMessageBubbleAction],
        answer: Chat.ProductMessageMenu?
    ) -> [ChatMessageBubbleAction] {
        switch answer {
        case nil:
            hostActions
        case let .custom(actions):
            actions.inMenuOrder.map { .product($0) }
        case let .extendBuiltIn(actions, hidden):
            hostActions.filter { action in
                action.builtIn.map { !hidden.contains($0) } ?? true
            } + actions.inMenuOrder.map { .product($0) }
        }
    }
}
