import CoreGraphics
import Foundation
import Metal

/// Loads everything the coin renderer draws with: the meshes, the struck-relief atlas, the studio
/// environment, and the constants the shader reads.
///
/// All of it is exported by `paritytech/coinage-viz` (`npm run export:native`, which writes
/// `public/native/assets`) and vendored under `CoinageAssets`. The constants arrive as data rather
/// than as code, so a material change is a file change here.
///
/// That project has done its job and nothing regenerates these files. Changing a coin face, a
/// shape or the studio means reproducing the export, and it needs four things that were never
/// upstreamed. Written down because they were lost once already: the CASH currency vanished with a
/// working copy, and recovering it meant reconstructing the definition and proving it by
/// reproducing the shipped atlas byte for byte before changing anything. That is also how to check
/// a reproduction — export at the settings of the day and compare bytes, then change one thing.
///
/// 1. `src/currency.js` gains
///    `cash: { id: "cash", symbol: "", decimals: 2, minor: null, name: "Cash" }`. No minor unit, so
///    every face is struck in the major unit with its decimals and the fifteen read as one series;
///    the reference's own currencies strike small values in the minor unit, which puts "16" and
///    "0.16" on two coins of the same value. The symbol is `""` rather than `null` because the
///    exported label is built by concatenation, and `null` reaches `designs.json` as "null0.01".
///
/// 2. `scripts/export-native.mjs` sets `NATIVE_TILE_PX = 256`, passes it to `createReliefAtlas` and
///    writes it into `params.json`. The atlas is four tiles square, so 1024². The web build's 512
///    is more than any coin here draws: the largest is sixty points, and the shader's own mip
///    selection never reaches the top level.
///
/// 3. The same script writes the relief as raw RGBA bytes, `relief/<cur>-relief.bin`, rather than
///    as a PNG. The alpha is a height, not an opacity, and CoreGraphics premultiplies RGBA —
///    silently scaling every normal by its own height. ``CoinageReliefAtlasTests`` guards it.
///
/// 4. The same script appends
///    `NATIVE_ONLY_GEOMETRIES = [{ kind: "flower", dials: defaultDials("flower"), core: 0 }]`
///    after `allDesigns()` in the mesh loop. Every metal band runs the same four outlines, so the
///    bimetallic band needs a flower with a core, which the reference's own denomination table
///    never asks for. Appended rather than added to that table, so `g0`–`g6` keep their numbers and
///    the new mesh is `g7`.
///
/// `metals.json` is not taken from a fresh export: it carries local tuning that separates bronze
/// further from gold.
final class CoinageAssetStore {
    struct Mesh {
        let positions: MTLBuffer
        let normals: MTLBuffer
        let surfaces: MTLBuffer
        let indices: MTLBuffer
        let indexCount: Int
    }

    enum Failure: Error {
        case missingAsset(String)
        case malformed(String)
        case deviceRefused(String)
    }

    let params: Params
    let metalRows: [Float]
    let designs: [Design]
    let reliefAtlas: MTLTexture
    let environment: MTLTexture

    private let meshes: [String: Mesh]

    init(device: MTLDevice, bundle: Bundle = .main) throws {
        let manifest: Manifest = try Self.decode("manifest", in: bundle)
        let raw: RawParams = try Self.decode("params", in: bundle)
        let metals: [RawMetal] = try Self.decode("metals", in: bundle)
        let rawDesigns: [RawDesign] = try Self.decode("designs", in: bundle)
        let rawMeshes: [RawMesh] = try Self.decode("meshes", in: bundle)

        params = Params(raw: raw, levels: CoinageStudioBake.levels)
        metalRows = metals.flatMap { $0.reflectance + [$0.roughness] + $0.tone + [$0.brush ?? 1] }
        designs = rawDesigns.map(Design.init(raw:))
        meshes = try rawMeshes.reduce(into: [:]) { loaded, entry in
            loaded[entry.id] = try Self.load(entry.mesh, device: device, bundle: bundle)
        }
        guard let atlas = manifest.atlases["cash"] else {
            throw Failure.malformed("manifest has no cash atlas")
        }

        reliefAtlas = try Self.loadReliefAtlas(device: device, bundle: bundle, atlas: atlas)
        environment = try Self.bakeStudio(device: device, bundle: bundle)
    }

    /// Every mesh, already loaded. A lookup, never a read.
    ///
    /// They used to load on first use, which put a file read and four buffer allocations inside a
    /// draw, and that is brutal when a field of coins all reach for one at once: every coin freezes
    /// while it is read, including the ones already moving.
    ///
    /// The seven come to under half a megabyte, which is cheaper to hold than to fetch at the wrong
    /// moment. The reference exports four levels of detail per shape; at the sizes drawn here the
    /// finest is a tenth of a pixel from the coarsest on the largest coin, so only one is shipped.
    func mesh(geometry: String) throws -> Mesh {
        guard let loaded = meshes[geometry] else { throw Failure.missingAsset("mesh \(geometry)") }

        return loaded
    }
}

// MARK: - Shader constants

extension CoinageAssetStore {
    /// `CoinParams` in `CoinageCoin.metal`, field for field and in order. All floats, then one int,
    /// so it goes to the GPU as a flat buffer.
    struct Params {
        var viewportWidth: Float = 0
        var viewportHeight: Float = 0
        var dpr: Float = 2
        let tilePixels: Float
        let environmentMaxLod: Float
        var lightTurn: SIMD3<Float> = .zero
        /// Every coin leaned the same way, for the debug dial; zero in use.
        var coinTurn: SIMD3<Float> = .zero
        let material: [Float]
        var backdrop: [Float] = [0, 0, 0]
        var debug: Int32 = 0
        var grain: Float = 1

        fileprivate init(raw: RawParams, levels: Int) {
            tilePixels = raw.atlas.tilePx
            environmentMaxLod = Float(levels - 1)
            material = [
                raw.material.exposure, raw.material.luster, raw.material.rimLuster,
                raw.material.lusterMinPx, raw.material.studioRadius, raw.material.studioScale,
                raw.material.rollGloss, raw.material.rollReliefHaze,
                raw.native.envSharpen, raw.material.haze, raw.material.polish,
                raw.material.tone, raw.material.grime, raw.material.engraveDark,
                raw.material.frost, raw.material.wallRough, raw.material.aaVariance,
                raw.material.aaThreshold, raw.material.reliefBias,
                raw.material.spinGrain, raw.material.spinMinPx, raw.material.rimPolish, raw.material.wallSatin,
                raw.material.brushCover, raw.material.brushDepth, raw.material.brushScale, raw.material.brushStretch,
                raw.material.wearDarken, raw.material.wearMatte, raw.material.brushMinPx,
                Tuning.reliefGain, Tuning.roomLift
            ] + Tuning.panels.prefix(Tuning.panelCount).flatMap(\.packed)
                + Array(repeating: 0, count: max(Tuning.panelCount - Tuning.panels.count, 0) * 5)
        }

        /// The struct's own alignment, from the `float2` it opens with. Metal rounds a struct up
        /// to it, and a device refuses a smaller buffer at the draw call with nothing to say why;
        /// the simulator does not check. Padding by the same rule keeps the two sizes equal.
        static let alignment = MemoryLayout<Float>.size * 2

        func packed() -> [Float] {
            var out: [Float] = [
                viewportWidth, viewportHeight, dpr, tilePixels, environmentMaxLod,
                lightTurn.x, lightTurn.y, lightTurn.z,
                coinTurn.x, coinTurn.y, coinTurn.z, grain
            ]
            out += material
            out += backdrop
            out.append(Float(bitPattern: UInt32(bitPattern: debug)))

            let stride = Self.alignment / MemoryLayout<Float>.size
            out.append(contentsOf: Array(repeating: 0, count: out.count % stride))

            return out
        }
    }

    /// Only what the renderer needs off a design: our own banding decides metal and shape, so the
    /// reference's own choices are deliberately not read.
    struct Design {
        let thickness: Float
        let tile: Float

        fileprivate init(raw: RawDesign) {
            thickness = raw.thickness
            tile = Float(raw.tile)
        }
    }
}

// MARK: - Loading

private extension CoinageAssetStore {
    struct MeshEntry: Decodable {
        struct Array: Decodable {
            let offset: Int
            let count: Int
            let components: Int
        }

        let file: String
        let layout: [String: Array]
    }

    struct RawMesh: Decodable {
        let id: String
        /// One level of detail per shape, which is all that is shipped.
        let mesh: MeshEntry

        private enum CodingKeys: String, CodingKey {
            case id
            case lods
        }

        init(from decoder: any Decoder) throws {
            let container = try decoder.container(keyedBy: CodingKeys.self)
            id = try container.decode(String.self, forKey: .id)

            let lods = try container.decode([String: MeshEntry].self, forKey: .lods)

            guard let only = lods["mid"] else {
                throw Failure.malformed("mesh \(id) has no mid level")
            }

            mesh = only
        }
    }

    struct RawMetal: Decodable {
        /// Reflectance at normal incidence, `f0` in the exported file.
        let reflectance: [Float]
        let roughness: Float
        let tone: [Float]
        let brush: Float?

        enum CodingKeys: String, CodingKey {
            case reflectance = "f0"
            case roughness
            case tone
            case brush
        }
    }

    struct RawDesign: Decodable {
        let thickness: Float
        let tile: Int
    }

    struct RawParams: Decodable {
        struct Atlas: Decodable {
            let tilePx: Float
        }

        struct Native: Decodable {
            let envSharpen: Float
        }

        struct Material: Decodable {
            let exposure, luster, rimLuster, lusterMinPx, studioRadius, studioScale: Float
            let rollGloss, rollReliefHaze, haze, polish, tone, grime: Float
            let engraveDark, frost, wallRough, aaVariance, aaThreshold, reliefBias: Float
            let spinGrain, spinMinPx, rimPolish, wallSatin: Float
            let brushCover, brushDepth, brushScale, brushStretch, wearDarken, wearMatte, brushMinPx: Float
        }

        let atlas: Atlas
        let native: Native
        let material: Material
    }

    struct Manifest: Decodable {
        /// What the relief atlas is, rather than what this file hopes it is. The tile size is in
        /// `params.json` for the shader; the rest is here because only the loader needs it.
        struct Atlas: Decodable {
            let size: Int
            let channels: Int
        }

        let atlases: [String: Atlas]
    }

    static func decode<T: Decodable>(_ name: String, in bundle: Bundle) throws -> T {
        guard let url = bundle.url(forResource: name, withExtension: "json") else {
            throw Failure.missingAsset("\(name).json")
        }

        return try JSONDecoder().decode(T.self, from: Data(contentsOf: url))
    }

    static func load(_ entry: MeshEntry, device: MTLDevice, bundle: Bundle) throws -> Mesh {
        let name = (entry.file as NSString).lastPathComponent

        guard let url = bundle.url(
            forResource: (name as NSString).deletingPathExtension,
            withExtension: "bin"
        ) else {
            throw Failure.missingAsset(name)
        }

        let data = try Data(contentsOf: url)

        func buffer(_ key: String) throws -> (MTLBuffer, Int) {
            guard let array = entry.layout[key] else { throw Failure.malformed("layout \(key)") }

            let length = array.count * array.components * MemoryLayout<Float>.size

            guard array.offset + length <= data.count else {
                throw Failure.malformed("\(key) runs past the file")
            }

            let made: MTLBuffer? = data.withUnsafeBytes {
                device.makeBuffer(bytes: $0.baseAddress! + array.offset, length: length)
            }

            guard let made else { throw Failure.deviceRefused("mesh buffer \(key)") }

            return (made, array.count)
        }

        let indices = try buffer("index")

        return try Mesh(
            positions: buffer("position").0,
            normals: buffer("normal").0,
            surfaces: buffer("surf").0,
            indices: indices.0,
            indexCount: indices.1
        )
    }

    /// The relief atlas: the struck normal in RGB, the height in A, one tile per denomination.
    ///
    /// Raw bytes, exactly as the texture wants them, so loading is a file read and an upload. It
    /// used to arrive as two images — normal as RGB, height as greyscale — and be merged here, a
    /// scalar pass over four million texels that cost 450 ms of the second between tapping the card
    /// and seeing any coins, producing identical bytes on every launch from files that never change.
    ///
    /// Raw rather than a PNG of the merged result, which is the obvious thing to try: the alpha
    /// here is a height, not an opacity, and an image decoder is entitled to premultiply RGBA.
    /// CoreGraphics does, reporting `premultipliedLast` and handing back every normal scaled by its
    /// own height — a flat, quietly wrong relief rather than anything that fails.
    /// ``CoinageReliefAtlasTests`` keeps that door shut.
    ///
    /// The atlas is generated for CASH rather than taken from the reference's currencies: every
    /// value is struck in the major unit with its decimals and no mark, so the fifteen faces read
    /// as one series. The reference's own atlases strike small values in the minor unit, which puts
    /// "16" and "0.16" on two coins of the same value.
    static func loadReliefAtlas(device: MTLDevice, bundle: Bundle, atlas: Manifest.Atlas) throws -> MTLTexture {
        guard let url = bundle.url(forResource: "cash-relief", withExtension: "bin") else {
            throw Failure.missingAsset("cash-relief.bin")
        }

        let bytes = try Data(contentsOf: url)
        let expected = atlas.size * atlas.size * atlas.channels

        guard atlas.channels == 4, bytes.count == expected else {
            throw Failure.malformed("relief atlas is \(bytes.count) bytes, not \(expected)")
        }

        let descriptor = MTLTextureDescriptor.texture2DDescriptor(
            pixelFormat: .rgba8Unorm,
            width: atlas.size,
            height: atlas.size,
            mipmapped: true
        )
        descriptor.usage = [.shaderRead]

        guard let texture = device.makeTexture(descriptor: descriptor) else {
            throw Failure.deviceRefused("relief atlas")
        }

        bytes.withUnsafeBytes {
            texture.replace(
                region: MTLRegionMake2D(0, 0, atlas.size, atlas.size),
                mipmapLevel: 0,
                withBytes: $0.baseAddress!,
                bytesPerRow: atlas.size * atlas.channels
            )
        }

        return texture
    }

    /// The studio's lamps and room from `params.json`, baked into its cube on the GPU.
    static func bakeStudio(device: MTLDevice, bundle: Bundle) throws -> MTLTexture {
        let studio: CoinageStudioBake.Studio = try decode("params", in: bundle)

        guard let library = try? device.makeDefaultLibrary(bundle: bundle) else {
            throw Failure.deviceRefused("shader library")
        }

        return try CoinageStudioBake.bake(studio, device: device, library: library)
    }
}
