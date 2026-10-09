package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.coins

import android.content.res.AssetManager
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.util.concurrent.Callable
import java.util.concurrent.Executors

/**
 * Everything the coin renderer draws with, decoded once: the meshes, the struck-relief atlas, the studio
 * environment, the shaders and the constants the material reads.
 *
 * All of it is exported by the maintainers' internal coinage reference implementation and vendored under
 * `assets/coinage`. The constants arrive as data rather than as code, so a material change is a file change
 * here.
 *
 * That project has done its job and nothing regenerates these files. Changing a coin face, a shape or the
 * studio means reproducing the export, and it needs four things that were never upstreamed. Written down
 * because they were lost once already on iOS: the CASH currency vanished with a working copy, and recovering
 * it meant reconstructing the definition and proving it by reproducing the shipped atlas byte for byte
 * before changing anything.
 *
 * 1. `src/currency.js` gains `cash: { id: "cash", symbol: "", decimals: 2, minor: null, name: "Cash" }`. No
 *    minor unit, so every face is struck in the major unit with its decimals and the fifteen read as one
 *    series. The symbol is `""` rather than `null` because the exported label is built by concatenation.
 *
 * 2. `scripts/export-native.mjs` sets `NATIVE_TILE_PX = 256`, passes it to `createReliefAtlas` and writes it
 *    into `params.json`. The atlas is four tiles square, so 1024².
 *
 * 3. The same script writes the relief as raw RGBA bytes rather than as a PNG. The alpha is a height, not an
 *    opacity, and an image decoder is entitled to premultiply RGBA — which would silently scale every normal
 *    by its own height. See [CoinageAssetBundle.relief].
 *
 * 4. The same script appends
 *    `NATIVE_ONLY_GEOMETRIES = [{ kind: "flower", dials: defaultDials("flower"), core: 0 }]` after
 *    `allDesigns()` in the mesh loop, so the bimetallic band can have a flower with a core.
 *
 * `metals.json` is not taken from a fresh export: it carries local tuning that separates bronze from gold.
 */
object CoinageAssetStore {
    private const val ROOT = "coinage"

    /** Slices in the cube's own face order. */
    private val FACE_ORDER = listOf("px", "nx", "py", "ny", "pz", "nz")

    private val json = Json { ignoreUnknownKeys = true }

    /**
     * Decodes the whole bundle. Slow enough to belong off the main thread — the studio is forty-two Radiance
     * faces and decoding them is almost all of the cost.
     */
    fun load(assets: AssetManager): CoinageAssetBundle {
        val params = json.decodeFromString<RawParams>(text(assets, "params.json"))
        val metals = json.decodeFromString<List<RawMetal>>(text(assets, "metals.json"))
        val designs = json.decodeFromString<List<RawDesign>>(text(assets, "designs.json"))
        val meshes = json.decodeFromString<List<RawMesh>>(text(assets, "meshes.json"))
        val manifest = json.decodeFromString<RawManifest>(text(assets, "manifest.json"))

        val atlas = manifest.atlases["cash"] ?: error("manifest has no cash atlas")

        return CoinageAssetBundle(
            material = params.material.packed(params.native.envSharpen),
            tilePixels = params.atlas.tilePx,
            environmentMaxLod = (manifest.env.cube.levels.size - 1).toFloat(),
            metalRows = metals.rows(),
            designs = designs.map { CoinageAssetStore.Design(thickness = it.thickness, tile = it.tile.toFloat()) },
            meshes = meshes.associate { it.id to mesh(assets, it) },
            relief = relief(assets, atlas),
            environment = environment(assets, manifest.env.cube.levels),
            vertexSource = text(assets, "coin.vert"),
            fragmentSource = text(assets, "coin.frag")
        )
    }

    /**
     * Only what the renderer needs off a design: our own banding decides metal and shape, so the reference's
     * own choices are deliberately not read.
     */
    data class Design(val thickness: Float, val tile: Float)

    private fun text(assets: AssetManager, name: String): String =
        assets.open("$ROOT/$name").use { it.readBytes().toString(Charsets.UTF_8) }

    private fun bytes(assets: AssetManager, name: String): ByteArray =
        assets.open("$ROOT/$name").use { it.readBytes() }

    /** Six rows of two `vec4`: reflectance plus roughness, then the darker tone plus a pad. */
    private fun List<RawMetal>.rows(): FloatArray {
        val rows = FloatArray(size * 8)

        forEachIndexed { index, metal ->
            val base = index * 8
            metal.reflectance.forEachIndexed { channel, value -> rows[base + channel] = value }
            rows[base + 3] = metal.roughness
            metal.tone.forEachIndexed { channel, value -> rows[base + 4 + channel] = value }
            rows[base + 7] = 0f
        }

        return rows
    }

    private fun mesh(assets: AssetManager, raw: RawMesh): CoinageMesh {
        val entry = raw.lods["mid"] ?: error("mesh ${raw.id} has no mid level")
        val data = bytes(assets, entry.file)

        fun slice(key: String): ByteBuffer {
            val array = entry.layout[key] ?: error("mesh ${raw.id} has no $key layout")
            val length = array.count * array.components * Float.SIZE_BYTES

            require(array.offset + length <= data.size) { "$key runs past ${entry.file}" }

            return ByteBuffer.allocateDirect(length)
                .order(ByteOrder.LITTLE_ENDIAN)
                .put(data, array.offset, length)
                .apply { rewind() }
        }

        val indices = entry.layout["index"] ?: error("mesh ${raw.id} has no index layout")

        return CoinageMesh(
            positions = slice("position"),
            normals = slice("normal"),
            surfaces = slice("surf"),
            indices = slice("index"),
            indexCount = indices.count
        )
    }

    private fun relief(assets: AssetManager, atlas: RawAtlas): CoinageRelief {
        val data = bytes(assets, "relief/cash-relief.bin")
        val expected = atlas.size * atlas.size * atlas.channels

        require(atlas.channels == 4 && data.size == expected) {
            "relief atlas is ${data.size} bytes, not $expected"
        }

        return CoinageRelief(
            size = atlas.size,
            pixels = ByteBuffer.allocateDirect(data.size).order(ByteOrder.nativeOrder())
                .put(data)
                .apply { rewind() }
        )
    }

    /**
     * Decoded all at once rather than one after another. The faces are independent, decoding is the whole
     * cost, and doing them in turn was most of what was left of the wait to open the card.
     */
    private fun environment(assets: AssetManager, levels: List<RawLevel>): List<CoinageEnvironmentFace> {
        val jobs = levels.flatMap { level ->
            FACE_ORDER.mapIndexed { slice, face ->
                val path = level.faces[face] ?: error("level ${level.level} is missing $face")

                // The manifest names the faces under a `cube/` folder the export never wrote; only the file
                // name is real, and the vendored faces sit flat in `env/`.
                Triple(level.level, slice, "env/" + path.substringAfterLast('/'))
            }
        }

        val pool = Executors.newFixedThreadPool(
            minOf(Runtime.getRuntime().availableProcessors(), jobs.size).coerceAtLeast(1)
        )

        return try {
            pool.invokeAll(
                jobs.map { (level, slice, path) ->
                    Callable {
                        val decoded = CoinageRadianceImage.decode(bytes(assets, path))

                        CoinageEnvironmentFace(level, slice, decoded.width, decoded.height, decoded.pixels)
                    }
                }
            ).map { it.get() }
        } finally {
            pool.shutdown()
        }
    }

    @Serializable
    private data class RawMetal(
        @SerialName("f0") val reflectance: List<Float>,
        val roughness: Float,
        val tone: List<Float>
    )

    @Serializable
    private data class RawDesign(val thickness: Float, val tile: Int)

    @Serializable
    private data class RawArray(val offset: Int, val count: Int, val components: Int)

    @Serializable
    private data class RawMeshEntry(val file: String, val layout: Map<String, RawArray>)

    @Serializable
    private data class RawMesh(val id: String, val lods: Map<String, RawMeshEntry>)

    @Serializable
    private data class RawAtlasParams(val tilePx: Float)

    @Serializable
    private data class RawNative(val envSharpen: Float)

    @Serializable
    private data class RawMaterial(
        val exposure: Float,
        val luster: Float,
        val rimLuster: Float,
        val lusterMinPx: Float,
        val studioRadius: Float,
        val studioScale: Float,
        val basin: Float,
        val rollGloss: Float,
        val rollReliefHaze: Float,
        val haze: Float,
        val polish: Float,
        val tone: Float,
        val grime: Float,
        val engraveDark: Float,
        val frost: Float,
        val wallRough: Float,
        val aaVariance: Float,
        val aaThreshold: Float,
        val reliefBias: Float
    ) {
        /** `envSharpen` comes from the native block, and sits where the shader expects it. */
        fun packed(envSharpen: Float) = CoinageMaterial(
            exposure = exposure,
            luster = luster,
            rimLuster = rimLuster,
            lusterMinPx = lusterMinPx,
            studioRadius = studioRadius,
            studioScale = studioScale,
            basin = basin,
            rollGloss = rollGloss,
            rollReliefHaze = rollReliefHaze,
            envSharpen = envSharpen,
            haze = haze,
            polish = polish,
            tone = tone,
            grime = grime,
            engraveDark = engraveDark,
            frost = frost,
            wallRough = wallRough,
            aaVariance = aaVariance,
            aaThreshold = aaThreshold,
            reliefBias = reliefBias
        )
    }

    @Serializable
    private data class RawParams(
        val atlas: RawAtlasParams,
        val native: RawNative,
        val material: RawMaterial
    )

    @Serializable
    private data class RawLevel(val level: Int, val size: Int, val faces: Map<String, String>)

    @Serializable
    private data class RawCube(val levels: List<RawLevel>)

    @Serializable
    private data class RawEnvironment(val cube: RawCube)

    @Serializable
    private data class RawAtlas(val size: Int, val channels: Int)

    @Serializable
    private data class RawManifest(val env: RawEnvironment, val atlases: Map<String, RawAtlas>)
}

/** The material constants the shader reads, in the order `params.json` gives them. */
data class CoinageMaterial(
    val exposure: Float,
    val luster: Float,
    val rimLuster: Float,
    val lusterMinPx: Float,
    val studioRadius: Float,
    val studioScale: Float,
    val basin: Float,
    val rollGloss: Float,
    val rollReliefHaze: Float,
    val envSharpen: Float,
    val haze: Float,
    val polish: Float,
    val tone: Float,
    val grime: Float,
    val engraveDark: Float,
    val frost: Float,
    val wallRough: Float,
    val aaVariance: Float,
    val aaThreshold: Float,
    val reliefBias: Float
)

/** Interleaved float arrays as the exporter wrote them, ready to hand straight to a buffer object. */
class CoinageMesh(
    val positions: ByteBuffer,
    val normals: ByteBuffer,
    val surfaces: ByteBuffer,
    val indices: ByteBuffer,
    val indexCount: Int
)

/**
 * The struck relief: the normal in RGB, the height in A, one tile per denomination.
 *
 * Raw bytes, exactly as the texture wants them, so loading is a file read and an upload. Raw rather than a
 * PNG of the merged result, which is the obvious thing to try: the alpha here is a height, not an opacity,
 * and an image decoder is entitled to premultiply RGBA, handing back every normal scaled by its own height —
 * a flat, quietly wrong relief rather than anything that fails.
 */
class CoinageRelief(val size: Int, val pixels: ByteBuffer)

class CoinageEnvironmentFace(
    val level: Int,
    val slice: Int,
    val width: Int,
    val height: Int,
    val pixels: ByteBuffer
)

/** Everything decoded, before any of it has touched a GL context. */
class CoinageAssetBundle(
    val material: CoinageMaterial,
    val tilePixels: Float,
    val environmentMaxLod: Float,
    val metalRows: FloatArray,
    val designs: List<CoinageAssetStore.Design>,
    val meshes: Map<String, CoinageMesh>,
    val relief: CoinageRelief,
    val environment: List<CoinageEnvironmentFace>,
    val vertexSource: String,
    val fragmentSource: String
)
