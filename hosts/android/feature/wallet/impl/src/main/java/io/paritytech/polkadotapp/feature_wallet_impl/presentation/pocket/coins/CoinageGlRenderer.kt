package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.coins

import android.opengl.GLES20
import android.opengl.GLES30
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.nio.FloatBuffer

/**
 * Draws coins with the reference's own material: measured-optics metals lit against a prefiltered studio,
 * relief from a baked normal atlas, wear that hazes the fields and leaves grime in the struck figure.
 *
 * One instanced draw per geometry in use, which is five to seven in practice. Everything else is one pass.
 *
 * Owns GL objects, so every method here has to run on the thread holding the context.
 */
class CoinageGlRenderer private constructor(
    val designs: List<CoinageAssetStore.Design>,
    private val bundle: CoinageAssetBundle,
    private val program: Int,
    private val vertexArray: Int,
    private val meshes: Map<String, GpuMesh>,
    private val instanceBuffer: Int,
    private val reliefTexture: Int,
    private val environmentTexture: Int,
    private val uniforms: Uniforms
) {
    private var instances: FloatBuffer = newInstanceBuffer(INITIAL_COINS)

    fun draw(
        batches: List<CoinageBatch>,
        viewportWidth: Float,
        viewportHeight: Float,
        pixelWidth: Int,
        pixelHeight: Int,
        density: Float,
        light: CoinageTilt.Turn
    ) {
        GLES20.glViewport(0, 0, pixelWidth, pixelHeight)
        GLES20.glClearColor(0f, 0f, 0f, 0f)
        GLES20.glClearDepthf(1f)
        GLES20.glClear(GLES20.GL_COLOR_BUFFER_BIT or GLES20.GL_DEPTH_BUFFER_BIT)

        GLES20.glEnable(GLES20.GL_DEPTH_TEST)
        GLES20.glDepthFunc(GLES20.GL_LESS)
        GLES20.glDepthMask(true)
        GLES20.glEnable(GLES20.GL_CULL_FACE)
        GLES20.glCullFace(GLES20.GL_BACK)
        GLES20.glFrontFace(GLES20.GL_CCW)
        GLES20.glDisable(GLES20.GL_BLEND)

        GLES20.glUseProgram(program)
        GLES30.glBindVertexArray(vertexArray)

        bindUniforms(viewportWidth, viewportHeight, density, light)
        bindTextures()

        ensureCapacity(batches.sumOf { it.instances.size })

        for (batch in batches) {
            val mesh = meshes[batch.geometry] ?: continue

            if (batch.instances.isEmpty()) continue

            upload(batch.instances)
            bindMesh(mesh)
            bindInstanceAttributes()

            GLES30.glDrawElementsInstanced(
                GLES20.GL_TRIANGLES,
                mesh.indexCount,
                GLES20.GL_UNSIGNED_INT,
                0,
                batch.instances.size
            )
        }

        GLES30.glBindVertexArray(0)
    }

    fun release() {
        GLES20.glDeleteProgram(program)
        GLES30.glDeleteVertexArrays(1, intArrayOf(vertexArray), 0)
        GLES20.glDeleteBuffers(1, intArrayOf(instanceBuffer), 0)
        GLES20.glDeleteTextures(2, intArrayOf(reliefTexture, environmentTexture), 0)

        for (mesh in meshes.values) {
            GLES20.glDeleteBuffers(4, intArrayOf(mesh.positions, mesh.normals, mesh.surfaces, mesh.indices), 0)
        }
    }

    private fun bindUniforms(width: Float, height: Float, density: Float, light: CoinageTilt.Turn) {
        val material = bundle.material

        GLES20.glUniform2f(uniforms.viewport, width, height)
        GLES20.glUniform1f(uniforms.dpr, density)
        GLES20.glUniform1f(uniforms.tilePx, bundle.tilePixels)
        GLES20.glUniform1f(uniforms.envMaxLod, bundle.environmentMaxLod)
        GLES20.glUniform3f(uniforms.lightTurn, light.x.toFloat(), light.y.toFloat(), light.z.toFloat())
        GLES20.glUniform1f(uniforms.exposure, material.exposure)
        GLES20.glUniform1f(uniforms.luster, material.luster)
        GLES20.glUniform1f(uniforms.rimLuster, material.rimLuster)
        GLES20.glUniform1f(uniforms.lusterMinPx, material.lusterMinPx)
        GLES20.glUniform1f(uniforms.studioRadius, material.studioRadius)
        GLES20.glUniform1f(uniforms.studioScale, material.studioScale)
        GLES20.glUniform1f(uniforms.basin, material.basin)
        GLES20.glUniform1f(uniforms.rollGloss, material.rollGloss)
        GLES20.glUniform1f(uniforms.rollReliefHaze, material.rollReliefHaze)
        GLES20.glUniform1f(uniforms.envSharpen, material.envSharpen)
        GLES20.glUniform1f(uniforms.haze, material.haze)
        GLES20.glUniform1f(uniforms.polish, material.polish)
        GLES20.glUniform1f(uniforms.tone, material.tone)
        GLES20.glUniform1f(uniforms.grime, material.grime)
        GLES20.glUniform1f(uniforms.engraveDark, material.engraveDark)
        GLES20.glUniform1f(uniforms.frost, material.frost)
        GLES20.glUniform1f(uniforms.wallRough, material.wallRough)
        GLES20.glUniform1f(uniforms.aaVariance, material.aaVariance)
        GLES20.glUniform1f(uniforms.aaThreshold, material.aaThreshold)
        GLES20.glUniform1f(uniforms.reliefBias, material.reliefBias)
        GLES20.glUniform3f(uniforms.backdrop, 0f, 0f, 0f)
        GLES20.glUniform1i(uniforms.debug, 0)
        GLES20.glUniform4fv(uniforms.metals, bundle.metalRows.size / 4, bundle.metalRows, 0)
    }

    private fun bindTextures() {
        GLES20.glActiveTexture(GLES20.GL_TEXTURE0)
        GLES20.glBindTexture(GLES20.GL_TEXTURE_2D, reliefTexture)
        GLES20.glUniform1i(uniforms.atlas, 0)

        GLES20.glActiveTexture(GLES20.GL_TEXTURE1)
        GLES20.glBindTexture(GLES20.GL_TEXTURE_CUBE_MAP, environmentTexture)
        GLES20.glUniform1i(uniforms.environment, 1)
    }

    private fun bindMesh(mesh: GpuMesh) {
        attribute(POSITION, mesh.positions, components = 3)
        attribute(NORMAL, mesh.normals, components = 3)
        attribute(SURF, mesh.surfaces, components = 4)

        GLES20.glBindBuffer(GLES20.GL_ELEMENT_ARRAY_BUFFER, mesh.indices)
    }

    private fun attribute(location: Int, buffer: Int, components: Int) {
        GLES20.glBindBuffer(GLES20.GL_ARRAY_BUFFER, buffer)
        GLES20.glEnableVertexAttribArray(location)
        GLES20.glVertexAttribPointer(location, components, GLES20.GL_FLOAT, false, 0, 0)
        GLES30.glVertexAttribDivisor(location, 0)
    }

    /** Five `vec4` interleaved, one per coin, advancing once per instance rather than per vertex. */
    private fun bindInstanceAttributes() {
        GLES20.glBindBuffer(GLES20.GL_ARRAY_BUFFER, instanceBuffer)

        for (slot in 0 until INSTANCE_VECTORS) {
            val location = INSTANCE_BASE + slot

            GLES20.glEnableVertexAttribArray(location)
            GLES20.glVertexAttribPointer(
                location,
                4,
                GLES20.GL_FLOAT,
                false,
                INSTANCE_STRIDE,
                slot * 4 * Float.SIZE_BYTES
            )
            GLES30.glVertexAttribDivisor(location, 1)
        }
    }

    /**
     * Enough room for every coin in the frame, grown if a wallet outgrows it. A fixed ceiling meant a large
     * enough wallet silently lost whatever did not fit.
     */
    private fun ensureCapacity(coins: Int) {
        if (instances.capacity() >= coins * INSTANCE_FLOATS) return

        instances = newInstanceBuffer(coins * 2)
    }

    private fun upload(batch: List<CoinageInstance>) {
        instances.clear()

        for (instance in batch) {
            instances.put(instance.positionX)
            instances.put(instance.positionY)
            instances.put(instance.positionZ)
            instances.put(instance.height)
            instances.put(instance.turn)
            instances.put(instance.tilt)
            instances.put(instance.spin)
            instances.put(instance.thickness)
            instances.put(instance.wear)
            instances.put(instance.outerMetal)
            instances.put(instance.coreMetal)
            instances.put(instance.tile)
            instances.put(instance.reeds)
            instances.put(instance.luster)
            instances.put(instance.recede)
            instances.put(instance.calm)
            instances.put(instance.pits)
            instances.put(instance.streaks)
            instances.put(instance.seed)
            instances.put(0f)
        }

        instances.flip()

        GLES20.glBindBuffer(GLES20.GL_ARRAY_BUFFER, instanceBuffer)
        // Orphaned rather than sub-updated: the driver hands back fresh storage instead of stalling on the
        // copy the last frame is still reading.
        GLES20.glBufferData(
            GLES20.GL_ARRAY_BUFFER,
            batch.size * INSTANCE_STRIDE,
            instances,
            GLES30.GL_STREAM_DRAW
        )
    }

    private class GpuMesh(
        val positions: Int,
        val normals: Int,
        val surfaces: Int,
        val indices: Int,
        val indexCount: Int
    )

    private class Uniforms(program: Int) {
        val viewport = GLES20.glGetUniformLocation(program, "uViewport")
        val dpr = GLES20.glGetUniformLocation(program, "uDpr")
        val tilePx = GLES20.glGetUniformLocation(program, "uTilePx")
        val envMaxLod = GLES20.glGetUniformLocation(program, "uEnvMaxLod")
        val lightTurn = GLES20.glGetUniformLocation(program, "uLightTurn")
        val exposure = GLES20.glGetUniformLocation(program, "uExposure")
        val luster = GLES20.glGetUniformLocation(program, "uLuster")
        val rimLuster = GLES20.glGetUniformLocation(program, "uRimLuster")
        val lusterMinPx = GLES20.glGetUniformLocation(program, "uLusterMinPx")
        val studioRadius = GLES20.glGetUniformLocation(program, "uStudioRadius")
        val studioScale = GLES20.glGetUniformLocation(program, "uStudioScale")
        val basin = GLES20.glGetUniformLocation(program, "uBasin")
        val rollGloss = GLES20.glGetUniformLocation(program, "uRollGloss")
        val rollReliefHaze = GLES20.glGetUniformLocation(program, "uRollReliefHaze")
        val envSharpen = GLES20.glGetUniformLocation(program, "uEnvSharpen")
        val haze = GLES20.glGetUniformLocation(program, "uHaze")
        val polish = GLES20.glGetUniformLocation(program, "uPolish")
        val tone = GLES20.glGetUniformLocation(program, "uTone")
        val grime = GLES20.glGetUniformLocation(program, "uGrime")
        val engraveDark = GLES20.glGetUniformLocation(program, "uEngraveDark")
        val frost = GLES20.glGetUniformLocation(program, "uFrost")
        val wallRough = GLES20.glGetUniformLocation(program, "uWallRough")
        val aaVariance = GLES20.glGetUniformLocation(program, "uAaVariance")
        val aaThreshold = GLES20.glGetUniformLocation(program, "uAaThreshold")
        val reliefBias = GLES20.glGetUniformLocation(program, "uReliefBias")
        val backdrop = GLES20.glGetUniformLocation(program, "uBackdrop")
        val debug = GLES20.glGetUniformLocation(program, "uDebug")
        val metals = GLES20.glGetUniformLocation(program, "uMetals")
        val atlas = GLES20.glGetUniformLocation(program, "uAtlas")
        val environment = GLES20.glGetUniformLocation(program, "uEnv")
    }

    companion object {
        private const val POSITION = 0
        private const val NORMAL = 1
        private const val SURF = 2
        private const val INSTANCE_BASE = 3
        private const val INSTANCE_VECTORS = 5
        private const val INSTANCE_FLOATS = INSTANCE_VECTORS * 4
        private const val INSTANCE_STRIDE = INSTANCE_FLOATS * 4

        /** What the instance buffer starts at. It grows to fit rather than dropping coins on the floor. */
        private const val INITIAL_COINS = 600

        /**
         * Builds every GL object from an already decoded bundle. Cheap — uploads only — which is why the
         * decode is shared across contexts and this is not.
         */
        fun create(bundle: CoinageAssetBundle): CoinageGlRenderer {
            val program = link(bundle.vertexSource, bundle.fragmentSource)
            val arrays = IntArray(1).also { GLES30.glGenVertexArrays(1, it, 0) }
            val instanceBuffer = IntArray(1).also { GLES20.glGenBuffers(1, it, 0) }[0]

            return CoinageGlRenderer(
                designs = bundle.designs,
                bundle = bundle,
                program = program,
                vertexArray = arrays[0],
                meshes = bundle.meshes.mapValues { (_, mesh) -> upload(mesh) },
                instanceBuffer = instanceBuffer,
                reliefTexture = uploadRelief(bundle.relief),
                environmentTexture = uploadEnvironment(bundle.environment),
                uniforms = Uniforms(program)
            )
        }

        private fun newInstanceBuffer(coins: Int): FloatBuffer =
            ByteBuffer.allocateDirect(maxOf(coins, 1) * INSTANCE_STRIDE)
                .order(ByteOrder.nativeOrder())
                .asFloatBuffer()

        private fun upload(mesh: CoinageMesh): GpuMesh {
            fun buffer(target: Int, data: ByteBuffer): Int {
                val name = IntArray(1).also { GLES20.glGenBuffers(1, it, 0) }[0]

                data.rewind()
                GLES20.glBindBuffer(target, name)
                GLES20.glBufferData(target, data.remaining(), data, GLES20.GL_STATIC_DRAW)

                return name
            }

            return GpuMesh(
                positions = buffer(GLES20.GL_ARRAY_BUFFER, mesh.positions),
                normals = buffer(GLES20.GL_ARRAY_BUFFER, mesh.normals),
                surfaces = buffer(GLES20.GL_ARRAY_BUFFER, mesh.surfaces),
                indices = buffer(GLES20.GL_ELEMENT_ARRAY_BUFFER, mesh.indices),
                indexCount = mesh.indexCount
            )
        }

        /** The atlas ships without mips, and the box filter has to be the GPU's. */
        private fun uploadRelief(relief: CoinageRelief): Int {
            val name = IntArray(1).also { GLES20.glGenTextures(1, it, 0) }[0]
            val levels = Integer.numberOfTrailingZeros(relief.size) + 1

            GLES20.glBindTexture(GLES20.GL_TEXTURE_2D, name)
            GLES30.glTexStorage2D(GLES20.GL_TEXTURE_2D, levels, GLES30.GL_RGBA8, relief.size, relief.size)

            relief.pixels.rewind()
            GLES20.glTexSubImage2D(
                GLES20.GL_TEXTURE_2D,
                0,
                0,
                0,
                relief.size,
                relief.size,
                GLES20.GL_RGBA,
                GLES20.GL_UNSIGNED_BYTE,
                relief.pixels
            )
            GLES20.glGenerateMipmap(GLES20.GL_TEXTURE_2D)
            sample(GLES20.GL_TEXTURE_2D)

            return name
        }

        /**
         * Every level of the prefiltered chain is uploaded, so a rough metal reads a blurred studio without
         * the shader doing the blurring.
         */
        private fun uploadEnvironment(faces: List<CoinageEnvironmentFace>): Int {
            val name = IntArray(1).also { GLES20.glGenTextures(1, it, 0) }[0]
            val base = faces.first { it.level == 0 }
            val levels = faces.maxOf { it.level } + 1

            GLES20.glBindTexture(GLES20.GL_TEXTURE_CUBE_MAP, name)
            GLES30.glTexStorage2D(
                GLES20.GL_TEXTURE_CUBE_MAP,
                levels,
                GLES30.GL_RGBA16F,
                base.width,
                base.height
            )

            for (face in faces) {
                face.pixels.rewind()
                GLES20.glTexSubImage2D(
                    GLES20.GL_TEXTURE_CUBE_MAP_POSITIVE_X + face.slice,
                    face.level,
                    0,
                    0,
                    face.width,
                    face.height,
                    GLES20.GL_RGBA,
                    GLES30.GL_HALF_FLOAT,
                    face.pixels
                )
            }

            sample(GLES20.GL_TEXTURE_CUBE_MAP)

            return name
        }

        private fun sample(target: Int) {
            GLES20.glTexParameteri(target, GLES20.GL_TEXTURE_MIN_FILTER, GLES20.GL_LINEAR_MIPMAP_LINEAR)
            GLES20.glTexParameteri(target, GLES20.GL_TEXTURE_MAG_FILTER, GLES20.GL_LINEAR)
            GLES20.glTexParameteri(target, GLES20.GL_TEXTURE_WRAP_S, GLES20.GL_CLAMP_TO_EDGE)
            GLES20.glTexParameteri(target, GLES20.GL_TEXTURE_WRAP_T, GLES20.GL_CLAMP_TO_EDGE)
        }

        private fun link(vertexSource: String, fragmentSource: String): Int {
            val program = GLES20.glCreateProgram()

            GLES20.glAttachShader(program, compile(GLES20.GL_VERTEX_SHADER, vertexSource))
            GLES20.glAttachShader(program, compile(GLES20.GL_FRAGMENT_SHADER, fragmentSource))
            GLES20.glLinkProgram(program)

            val status = IntArray(1)
            GLES20.glGetProgramiv(program, GLES20.GL_LINK_STATUS, status, 0)

            check(status[0] != 0) { "coin program did not link: ${GLES20.glGetProgramInfoLog(program)}" }

            return program
        }

        private fun compile(type: Int, source: String): Int {
            val shader = GLES20.glCreateShader(type)

            GLES20.glShaderSource(shader, source)
            GLES20.glCompileShader(shader)

            val status = IntArray(1)
            GLES20.glGetShaderiv(shader, GLES20.GL_COMPILE_STATUS, status, 0)

            check(status[0] != 0) { "coin shader did not compile: ${GLES20.glGetShaderInfoLog(shader)}" }

            return shader
        }
    }
}
