import Testing
import UIKit

@testable import polkadot_app

struct BlurHashMediaPreviewProviderTests {
    @Test("Invalid thumbnail bytes fall through to the media provider")
    @MainActor
    func invalidThumbnailData() {
        let mediaProvider = ChatMessageMediaPreviewProviderMock()
        let provider = BlurHashMediaPreviewProvider(
            thumbnail: Data([0xFF]),
            dataExistencePredicate: { false },
            mediaProvider: mediaProvider
        )

        provider.providePreview(for: UIImageView(), size: CGSize(width: 24, height: 16))

        #expect(mediaProvider.didProvidePreview)
    }
}
