import Foundation
import Metal
import Testing

@testable import polkadot_app

/// The relief atlas carries data in a texture's channels: the struck normal in RGB, the height in
/// A. Nothing downstream reports it if those arrive altered — the coins just look flat — so what
/// the file holds is pinned here.
@Suite("Coin relief atlas")
struct CoinageReliefAtlasTests {
    private func atlasBytes() throws -> [UInt8] {
        let url = try #require(
            Bundle.main.url(forResource: "cash-relief", withExtension: "bin"),
            "the relief atlas is not in the bundle"
        )

        return try [UInt8](Data(contentsOf: url))
    }

    private func store() throws -> CoinageAssetStore {
        try CoinageAssetStore(device: #require(MTLCreateSystemDefaultDevice()))
    }

    @Test("The atlas holds a whole number of tiles at the size the shader is told")
    func tilesFitTheAtlas() throws {
        let store = try store()
        let side = store.reliefAtlas.width

        #expect(store.reliefAtlas.height == side)
        // ATLAS_TILES in CoinageCoin.metal, which the shader uses to find a tile's corner.
        #expect(Int(store.params.tilePixels) * 4 == side)
        #expect(try atlasBytes().count == side * side * 4)
    }

    @Test("The normals keep their length, so nothing has premultiplied them by the height")
    func normalsAreUnitLength() throws {
        let bytes = try atlasBytes()
        var checked = 0

        // Every 997th texel: a prime stride, so the sample crosses tiles rather than landing in the
        // same place in each.
        for texel in stride(from: 0, to: bytes.count / 4, by: 997) {
            let normal = (0 ..< 3).map { Double(bytes[texel * 4 + $0]) / 255 * 2 - 1 }
            let length = sqrt(normal.reduce(0) { $0 + $1 * $1 })

            // A byte per channel cannot land exactly on one, but it lands close. Premultiplying a
            // flat texel — normal (128, 128, 255) under a height of 128 — gives 0.70 instead.
            #expect(abs(length - 1) < 0.02)
            checked += 1
        }

        #expect(checked > 1_000)
    }

    @Test("A face's flat field reads as pointing straight out of the coin")
    func theFieldPointsOutward() throws {
        let bytes = try atlasBytes()
        let side = try store().reliefAtlas.width
        // Just inside the first tile's edge, which is field rather than struck figure.
        let texel = (side / 32) * side + side / 32
        let normal = (0 ..< 3).map { Double(bytes[texel * 4 + $0]) / 255 * 2 - 1 }

        #expect(abs(normal[0]) < 0.02)
        #expect(abs(normal[1]) < 0.02)
        #expect(normal[2] > 0.98)
    }
}
