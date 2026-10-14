import UIKit
import PolkadotUI

final class ChatMessageMediaPreviewProviderMock: ChatMessageMediaPreviewProviding {
    var identifier: String { "chat-message-media-preview-provider-mock" }
    private(set) var didProvidePreview = false

    @MainActor
    func providePreview(for _: UIImageView, size _: CGSize?) {
        didProvidePreview = true
    }
}
