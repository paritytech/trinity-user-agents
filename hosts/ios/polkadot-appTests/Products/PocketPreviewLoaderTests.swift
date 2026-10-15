import Foundation
import Products
import Testing
@testable import polkadot_app

struct PocketPreviewLoaderTests {
    @Test
    func readsAFaceFromTheProductArchive() async throws {
        let loader = PocketPreviewLoader(archive: StubArchive(contents: minimalFace), fetch: refusingFetch)

        let face = try await loader.load(contentId: "worker.game.paseo", preview: .archive(path: "faces/loyalty.json"))

        guard case .column = face else {
            Issue.record("expected a column, got \(face)")
            return
        }
    }

    /// The preview is read before the user has approved anything, so how much
    /// there is to read is the product's choice. Its size is checked rather than
    /// its content: a hostile face that has been decoded is already held whole.
    @Test
    func refusesAnArchiveFaceBeyondTheSizeBound() async {
        let oversized = Data(repeating: UInt8(ascii: " "), count: 256 * 1_024 + 1)
        let loader = PocketPreviewLoader(archive: StubArchive(contents: oversized), fetch: refusingFetch)

        await #expect(throws: (any Error).self) {
            try await loader.load(contentId: "worker.game.paseo", preview: .archive(path: "faces/big.json"))
        }
    }

    /// The reader is told the bound rather than being left to hand back
    /// whatever the product published for the loader to measure afterwards.
    @Test
    func tellsTheArchiveHowMuchItMayRead() async throws {
        let archive = StubArchive(contents: minimalFace)
        let loader = PocketPreviewLoader(archive: archive, fetch: refusingFetch)

        _ = try await loader.load(contentId: "worker.game.paseo", preview: .archive(path: "faces/loyalty.json"))

        #expect(archive.boundedTo == PocketPreviewLoader.maxBytes)
    }

    @Test
    func refusesAFaceTheArchiveDoesNotHold() async {
        let loader = PocketPreviewLoader(archive: StubArchive(contents: nil), fetch: refusingFetch)

        await #expect(throws: (any Error).self) {
            try await loader.load(contentId: "worker.game.paseo", preview: .archive(path: "faces/missing.json"))
        }
    }

    /// A URL preview only ever comes from the debug menu, and is bounded the
    /// same way an archive one is.
    @Test
    func readsAFaceFromADebugUrl() async throws {
        let loader = PocketPreviewLoader(
            archive: StubArchive(contents: nil),
            fetch: { _, _ in minimalFace }
        )

        let face = try await loader.load(
            contentId: "worker.game.paseo",
            preview: .url("http://127.0.0.1:5173/face.json")
        )

        guard case .column = face else {
            Issue.record("expected a column, got \(face)")
            return
        }
    }

    @Test
    func refusesAUrlFaceBeyondTheSizeBound() async {
        let oversized = Data(repeating: UInt8(ascii: " "), count: 256 * 1_024 + 1)
        let loader = PocketPreviewLoader(archive: StubArchive(contents: nil), fetch: { _, _ in oversized })

        await #expect(throws: (any Error).self) {
            try await loader.load(contentId: "worker.game.paseo", preview: .url("http://127.0.0.1:5173/big.json"))
        }
    }
}
