import SwiftUI

public protocol WidgetNodeProviding: AnyObject, Observable {
    @MainActor var node: CustomMessageWidgetNode? { get }

    /// A view shows the body. A provider that streams the body opens its stream now.
    @MainActor func bodyAppeared()

    /// A view stops to show the body. A provider closes its stream when no view shows the body, so the stream
    /// does not keep the worker of the product (02-DESIGN §5.2).
    @MainActor func bodyDisappeared()
}

public extension WidgetNodeProviding {
    @MainActor func bodyAppeared() {}
    @MainActor func bodyDisappeared() {}
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
        Group {
            if let node = nodeProvider.node {
                CustomMessageWidgetView(node: node, onAction: onAction, resolveImage: resolveImage)
                    .frame(maxWidth: .infinity, alignment: .leading)
            } else {
                ProgressView()
                    .frame(maxWidth: .infinity, minHeight: 44)
            }
        }
        .onAppear { nodeProvider.bodyAppeared() }
        .onDisappear { nodeProvider.bodyDisappeared() }
    }

    public static func == (lhs: Self, rhs: Self) -> Bool { lhs.messageId == rhs.messageId }
    public func hash(into hasher: inout Hasher) { hasher.combine(messageId) }
}
