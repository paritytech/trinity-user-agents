import SwiftUI

public protocol WidgetNodeProviding: AnyObject, Observable {
    @MainActor var node: CustomMessageWidgetNode? { get }
}

public struct ProductWidgetChatView: View, Hashable {
    let messageId: String
    let nodeProvider: any WidgetNodeProviding
    let onAction: WidgetActionHandler?
    let resolveImage: WidgetImageResolver

    public init(
        messageId: String,
        nodeProvider: any WidgetNodeProviding,
        onAction: WidgetActionHandler?,
        resolveImage: WidgetImageResolver
    ) {
        self.messageId = messageId
        self.nodeProvider = nodeProvider
        self.onAction = onAction
        self.resolveImage = resolveImage
    }

    public var body: some View {
        if let node = nodeProvider.node {
            CustomMessageWidgetView(node: node, onAction: onAction, resolveImage: resolveImage)
                .frame(maxWidth: .infinity, alignment: .leading)
        } else {
            ProgressView()
                .frame(maxWidth: .infinity, minHeight: 44)
        }
    }

    public static func == (lhs: Self, rhs: Self) -> Bool { lhs.messageId == rhs.messageId }
    public func hash(into hasher: inout Hasher) { hasher.combine(messageId) }
}
