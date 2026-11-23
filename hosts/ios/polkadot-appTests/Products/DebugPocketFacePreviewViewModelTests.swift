import Foundation
import os
import Testing
@testable import polkadot_app

/// The preview is how a back is worked on before any worker draws it, so it draws the back from its own address, and
/// a preview with no back typed in stays the front alone.
@MainActor
struct DebugPocketFacePreviewViewModelTests {
    @Test(arguments: [("http://127.0.0.1:5173/faces/loyalty-back.json", true), ("  ", false)])
    func drawsTheBackFromItsOwnAddressOnlyWhenOneIsTyped(backUrl: String, drawsBack: Bool) async {
        let fetched = OSAllocatedUnfairLock(initialState: [URL]())
        let viewModel = DebugPocketFacePreviewViewModel(fetch: { url, _ in
            fetched.withLock { $0.append(url) }
            return minimalFace
        })
        viewModel.url = "http://127.0.0.1:5173/faces/loyalty.json"
        viewModel.backUrl = backUrl

        await viewModel.load()

        #expect(viewModel.face != nil && (viewModel.back != nil) == drawsBack)
        #expect(fetched.withLock { $0 }.count == (drawsBack ? 2 : 1))
    }
}
