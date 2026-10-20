import Foundation
import Testing
import UIKit
@testable import polkadot_app

/// Kingfisher's async retrieve never returns for the second of two overlapping loads of one URL, so a face that
/// shows the same image twice kept a spinner in one of them. Overlapping loads must share one.
struct InFlightImagesTests {
    @Test
    func overlappingLoadsOfOneAddressShareOneLoad() async throws {
        let images = InFlightImages()
        let loads = LoadCounter()
        let url = try #require(URL(string: "http://127.0.0.1:5173/images/stamp.png"))
        let load: @Sendable () async throws -> UIImage = {
            await loads.increment()
            try await Task.sleep(for: .milliseconds(50))
            return UIImage()
        }

        async let first = images.image(at: url, load: load)
        async let second = images.image(at: url, load: load)
        _ = try await (first, second)

        #expect(await loads.count == 1)
    }
}

private actor LoadCounter {
    private(set) var count = 0

    func increment() {
        count += 1
    }
}
