import Foundation
import PolkadotUI

extension MessageListModel {
    /// The room shows its placeholder only when the window holds the whole room: a product-source room that
    /// still loads its first page, or that did not answer, shows no placeholder (P6.8, 02-DESIGN §8.6).
    var showsPlaceholder: Bool {
        paging.older == .end && paging.newer == .end && readiness == .ready
    }
}

extension ChatViewModelFactory {
    /// The text of an empty room, when the window holds the whole room.
    func productRoomPlaceholder(
        model: MessageListModel,
        metadata: MessageListMetadata,
        sections: [ChatViewLayout.Section]
    ) -> String? {
        guard model.showsPlaceholder else { return nil }

        let text = placeholderText(for: metadata.chatMetadata)
        if sections.isEmpty, let text {
            MainActor.assumeIsolated { productRoom?.mark("placeholder", text) }
        }
        return text
    }
}
