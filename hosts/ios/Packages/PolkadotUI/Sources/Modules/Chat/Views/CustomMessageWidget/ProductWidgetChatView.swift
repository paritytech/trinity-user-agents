import SwiftUI

public protocol WidgetNodeProviding: AnyObject, Observable {
    @MainActor var node: CustomMessageWidgetNode? { get }
    /// Offsets the product set on keyed gestures since its last tree; nil when it cannot set any.
    var gestureOffsets: GestureOffsets? { get }
}

public extension WidgetNodeProviding {
    var gestureOffsets: GestureOffsets? { nil }
}

public struct ProductWidgetChatView: View, Hashable {
    let messageId: String
    let nodeProvider: any WidgetNodeProviding
    let onAction: WidgetActionHandler?
    let resolveImage: WidgetImageResolver
    let materials: WidgetMaterials?

    public init(
        messageId: String,
        nodeProvider: any WidgetNodeProviding,
        onAction: WidgetActionHandler?,
        resolveImage: WidgetImageResolver,
        materials: WidgetMaterials? = nil
    ) {
        self.messageId = messageId
        self.nodeProvider = nodeProvider
        self.onAction = onAction
        self.resolveImage = resolveImage
        self.materials = materials
    }

    public var body: some View {
        if let node = nodeProvider.node {
            CustomMessageWidgetView(node: node, onAction: onAction, resolveImage: resolveImage, materials: materials)
                .environment(\.gestureOffsets, nodeProvider.gestureOffsets)
                .frame(maxWidth: .infinity, alignment: .leading)
        } else {
            ProgressView()
                .frame(maxWidth: .infinity, minHeight: 44)
        }
    }

    public static func == (lhs: Self, rhs: Self) -> Bool { lhs.messageId == rhs.messageId }
    public func hash(into hasher: inout Hasher) { hasher.combine(messageId) }
}
