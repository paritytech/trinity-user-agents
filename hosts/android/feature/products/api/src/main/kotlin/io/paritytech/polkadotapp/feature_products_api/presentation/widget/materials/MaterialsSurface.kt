package io.paritytech.polkadotapp.feature_products_api.presentation.widget.materials

import android.animation.ValueAnimator
import android.graphics.Bitmap
import android.opengl.EGLSurface
import android.opengl.GLES30
import android.view.Choreographer
import android.view.Surface
import io.paritytech.polkadotapp.common.utils.logFailure
import uniffi.truapi.ImageRef
import uniffi.truapi.MaterialFailure
import uniffi.truapi.Parameter
import uniffi.truapi.ReadyLayer
import uniffi.truapi.TextureRole
import uniffi.truapi.UniformKind
import uniffi.truapi.UniformRole
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.util.concurrent.atomic.AtomicBoolean
import kotlin.time.Duration.Companion.nanoseconds

/**
 * Draws one node's shader layers into a window surface on [gl]'s thread. Callable from any thread; the
 * callbacks run on the GL thread.
 */
class MaterialsSurface(
    private val gl: MaterialsGl,
    private val onFirstFrame: () -> Unit,
    private val onFailure: (material: ByteArray?, reason: MaterialFailure) -> Unit,
    private val onTooSlow: () -> Unit,
) : Choreographer.FrameCallback {
    private val released = AtomicBoolean(false)

    private val schedule = FrameSchedule()
    private val budget = GpuBudget()
    private val inputs = MaterialInputs()
    private val sensor = DeviceOrientationSensor(gl.applicationContext, gl.handler) { rotation, hasHeading ->
        reading = inputs.angles(rotation)
        readingRotation = if (hasHeading) rotation.copyOf() else null
        schedule.onChange()
        requestFrame()
    }
    private var reading: DeviceAngles? = null
    private var readingRotation: FloatArray? = null
    private var reducedMotion = false

    private var window: EGLSurface? = null
    private var width = 0
    private var height = 0
    private var layers: List<ReadyLayer> = emptyList()
    private var images: Map<Pair<Int, Int>, Bitmap> = emptyMap()
    private var noise: ByteArray? = null
    private var base: Bitmap? = null

    private var quad = 0
    private var baseTexture = 0
    private var noiseTexture = 0
    private val imageTextures = mutableMapOf<Pair<Int, Int>, Int>()
    private val targets = arrayOfNulls<FloatTarget>(2)
    private var uploadedBase: Bitmap? = null
    private var uploadedNoise: ByteArray? = null
    private var uploadedImages: Map<Pair<Int, Int>, Bitmap> = emptyMap()
    private var uploadBuffer: ByteBuffer? = null
    private val argumentValues = FloatArray(MAX_ARGUMENT_FLOATS)

    private var scheduled = false
    private var broken = false
    private var showedFirstFrame = false
    private var attachedToGl = false

    fun attach(surface: Surface, width: Int, height: Int) = post {
        this.width = width
        this.height = height
        window = gl.windowSurface(surface)
        if (window == null) return@post fail(null, MaterialFailure.UNSUPPORTED)
        if (!attachedToGl) gl.surfaceAttached()
        attachedToGl = true
        reducedMotion = !ValueAnimator.areAnimatorsEnabled()
        schedule.resume(System.nanoTime())
        listenIfMoving()
        requestFrame()
    }

    fun resize(width: Int, height: Int) = post {
        this.width = width
        this.height = height
        schedule.onChange()
        requestFrame()
    }

    fun detach() = post {
        sensor.stop()
        window?.let(gl::destroy)
        window = null
    }

    fun setBase(bitmap: Bitmap) = post {
        base = bitmap
        schedule.onChange()
        requestFrame()
    }

    fun setLayers(layers: List<ReadyLayer>, images: Map<Pair<Int, Int>, Bitmap>, noise: ByteArray) = post {
        if (!layers.drawsSameAs(this.layers)) budget.reset()
        this.layers = layers
        this.images = images
        this.noise = noise
        listenIfMoving()
        schedule.onChange()
        requestFrame()
    }

    fun pause() = post {
        schedule.pause()
        sensor.stop()
    }

    fun resume() = post {
        reducedMotion = !ValueAnimator.areAnimatorsEnabled()
        schedule.resume(System.nanoTime())
        listenIfMoving()
        requestFrame()
    }

    // Not awaited: a material stuck on the GPU must not hold the main thread.
    fun release() {
        if (!released.compareAndSet(false, true)) return
        gl.handler.post {
            schedule.release()
            sensor.stop()
            Choreographer.getInstance().removeFrameCallback(this)
            if (attachedToGl) gl.surfaceDetached()
            attachedToGl = false
            runCatching {
                if (gl.makeCurrent(window) || gl.makeCurrent(null)) deleteGlObjects()
                window?.let(gl::destroy)
            }.logFailure("materials: a released node left GL objects behind")
            window = null
        }
    }

    private fun post(block: () -> Unit) {
        if (!released.get()) gl.handler.post { if (!released.get()) guarded(block) }
    }

    // A throw on the GL thread would end the process, so it fails this node instead.
    private fun guarded(block: () -> Unit) {
        runCatching(block)
            .logFailure("materials: a node failed on the GL thread and draws plain")
            .onFailure { fail(null, MaterialFailure.UNSUPPORTED) }
    }

    private val animating: Boolean
        get() = layers.any { it.shader.readsTime } && !reducedMotion

    private fun listenIfMoving() {
        if (layers.any { it.shader.readsMotion } && !reducedMotion) sensor.start() else sensor.stop()
    }

    private fun requestFrame() {
        if (scheduled || broken || window == null || !schedule.wantsFrame(animating)) return
        scheduled = true
        Choreographer.getInstance().postFrameCallback(this)
    }

    override fun doFrame(frameTimeNanos: Long) {
        scheduled = false
        if (released.get() || broken) return
        guarded {
            if (schedule.next(animating)) drawFrame(frameTimeNanos)
            requestFrame()
        }
    }

    private fun drawFrame(frameTimeNanos: Long) {
        val surface = window ?: return
        val baseImage = base ?: return
        if (layers.isEmpty() || width <= 0 || height <= 0) return
        if (!gl.makeCurrent(surface)) return fail(null, MaterialFailure.UNSUPPORTED)
        if (layers.size > 1 && !gl.floatTargets) return fail(layers[1].material, MaterialFailure.UNSUPPORTED)

        val programs = layers.map { layer ->
            gl.program(layer.material, layer.shader).getOrElse {
                return fail(layer.material, MaterialFailure.UNSUPPORTED)
            }
        }
        gl.clearErrors()
        upload(baseImage)
        val frame = MaterialInputs.frameInputs(
            seconds = schedule.seconds(frameTimeNanos),
            beta = reading?.beta,
            gamma = reading?.gamma,
            alpha = reading?.alpha?.takeIf { readingRotation != null },
            rotation = readingRotation,
            still = reducedMotion,
        )

        val timed = budget.timing
        val started = System.nanoTime()
        layers.forEachIndexed { index, layer -> drawLayer(index, layer, programs[index], frame) }
        if (timed) GLES30.glFinish()
        val frameTime = (System.nanoTime() - started).nanoseconds

        if (GLES30.glGetError() != GLES30.GL_NO_ERROR) return fail(null, MaterialFailure.UNSUPPORTED)
        if (!gl.swap(surface)) return
        if (!showedFirstFrame) {
            showedFirstFrame = true
            onFirstFrame()
        }
        if (timed && budget.record(frameTime)) {
            broken = true
            onTooSlow()
        }
    }

    private fun drawLayer(index: Int, layer: ReadyLayer, program: Int, frame: FrameInputs) {
        val last = index == layers.lastIndex
        if (last) {
            GLES30.glBindFramebuffer(GLES30.GL_FRAMEBUFFER, 0)
            GLES30.glClearColor(0f, 0f, 0f, 0f)
            GLES30.glClear(GLES30.GL_COLOR_BUFFER_BIT)
            GLES30.glEnable(GLES30.GL_BLEND)
            // Straight alpha out of the shader, premultiplied into the window the compositor blends.
            GLES30.glBlendFuncSeparate(GLES30.GL_SRC_ALPHA, GLES30.GL_ZERO, GLES30.GL_ONE, GLES30.GL_ZERO)
        } else {
            GLES30.glBindFramebuffer(GLES30.GL_FRAMEBUFFER, target(index).framebuffer)
            GLES30.glDisable(GLES30.GL_BLEND)
        }
        GLES30.glViewport(0, 0, width, height)
        GLES30.glDisable(GLES30.GL_CULL_FACE)
        GLES30.glUseProgram(program)

        layer.shader.textures.forEachIndexed { slot, texture ->
            GLES30.glActiveTexture(GLES30.GL_TEXTURE0 + slot)
            GLES30.glBindTexture(GLES30.GL_TEXTURE_2D, texture(index, layer, texture.role))
            GLES30.glUniform1i(GLES30.glGetUniformLocation(program, texture.name), slot)
        }
        layer.shader.uniforms.forEach { uniform ->
            val location = GLES30.glGetUniformLocation(program, uniform.name)
            if (location < 0) return@forEach
            when (val role = uniform.role) {
                UniformRole.Time -> GLES30.glUniform1f(location, frame.time)
                UniformRole.Tilt -> GLES30.glUniform2fv(location, 1, frame.tilt, 0)
                UniformRole.Resolution -> GLES30.glUniform2f(location, width.toFloat(), height.toFloat())
                UniformRole.Orientation -> GLES30.glUniform3fv(location, 1, frame.orientation, 0)
                UniformRole.Rotation -> GLES30.glUniformMatrix3fv(location, 1, true, frame.rotation, 0)
                is UniformRole.Argument -> setArgument(location, uniform.kind, layer.args.getOrNull(role.v1.toInt()))
                is UniformRole.SamplingMode -> {
                    val slotRole = layer.shader.textures.getOrNull(role.v1.toInt())?.role
                    GLES30.glUniform1i(location, if (index > 0 && slotRole == TextureRole.Input) 1 else 0)
                }
                UniformRole.TargetFlip -> GLES30.glUniform1f(location, if (last) 1f else -1f)
            }
        }

        val position = GLES30.glGetAttribLocation(program, "position")
        GLES30.glBindBuffer(GLES30.GL_ARRAY_BUFFER, quad())
        GLES30.glEnableVertexAttribArray(position)
        GLES30.glVertexAttribPointer(position, 2, GLES30.GL_FLOAT, false, 0, 0)
        GLES30.glDrawArrays(GLES30.GL_TRIANGLE_STRIP, 0, QUAD_VERTICES)
    }

    private fun texture(index: Int, layer: ReadyLayer, role: TextureRole): Int = when (role) {
        TextureRole.Base -> baseTexture
        TextureRole.Input -> if (index == 0) baseTexture else target(index - 1).texture
        TextureRole.Noise -> noiseTexture()
        TextureRole.Trail -> NO_TEXTURE
        is TextureRole.Argument -> when (layer.args.getOrNull(role.v1.toInt())?.imageRef()) {
            is ImageRef.Source -> imageTexture(index to role.v1.toInt())
            ImageRef.Noise, null -> noiseTexture()
        }
    }

    private fun setArgument(location: Int, kind: UniformKind, argument: Parameter?) {
        when (argument) {
            is Parameter.Float -> argumentValues[0] = Float.fromBits(argument.v1.toInt())
            is Parameter.Vec2 -> fillArguments(argument.v1)
            is Parameter.Vec3 -> fillArguments(argument.v1)
            is Parameter.Vec4 -> fillArguments(argument.v1)
            else -> return
        }
        when (kind) {
            UniformKind.FLOAT -> GLES30.glUniform1fv(location, 1, argumentValues, 0)
            UniformKind.VEC2 -> GLES30.glUniform2fv(location, 1, argumentValues, 0)
            UniformKind.VEC3 -> GLES30.glUniform3fv(location, 1, argumentValues, 0)
            UniformKind.VEC4 -> GLES30.glUniform4fv(location, 1, argumentValues, 0)
            UniformKind.INT, UniformKind.MAT3 -> Unit
        }
    }

    private fun fillArguments(bits: List<UInt>) {
        bits.forEachIndexed { index, value -> argumentValues[index] = Float.fromBits(value.toInt()) }
    }

    private fun upload(baseImage: Bitmap) {
        if (uploadedBase !== baseImage) {
            baseTexture = uploadBitmap(baseTexture, baseImage)
            uploadedBase = baseImage
        }
        if (uploadedImages !== images) {
            imageTextures.values.forEach { GLES30.glDeleteTextures(1, intArrayOf(it), 0) }
            imageTextures.clear()
            images.forEach { (slot, bitmap) -> imageTextures[slot] = uploadBitmap(0, bitmap) }
            uploadedImages = images
        }
    }

    private fun imageTexture(slot: Pair<Int, Int>): Int = imageTextures[slot] ?: noiseTexture()

    private fun noiseTexture(): Int {
        val image = noise ?: return 0
        if (uploadedNoise === image && noiseTexture != 0) return noiseTexture
        if (noiseTexture == 0) noiseTexture = newTexture(GLES30.GL_NEAREST, GLES30.GL_REPEAT)
        GLES30.glBindTexture(GLES30.GL_TEXTURE_2D, noiseTexture)
        GLES30.glPixelStorei(GLES30.GL_UNPACK_ALIGNMENT, 1)
        GLES30.glTexImage2D(
            GLES30.GL_TEXTURE_2D, 0, GLES30.GL_R8, NOISE_SIDE, NOISE_SIDE, 0,
            GLES30.GL_RED, GLES30.GL_UNSIGNED_BYTE, ByteBuffer.wrap(image),
        )
        uploadedNoise = image
        return noiseTexture
    }

    // Row 0 of the bitmap is its top, and the shaders read row 0 as the node's top.
    private fun uploadBitmap(existing: Int, bitmap: Bitmap): Int {
        val pixels = bitmap.takeIf { it.config == Bitmap.Config.ARGB_8888 }
            ?: checkNotNull(bitmap.copy(Bitmap.Config.ARGB_8888, false)) { "materials: no ARGB copy of an image" }
        val buffer = uploadBuffer?.takeIf { it.capacity() >= pixels.byteCount }
            ?: ByteBuffer.allocateDirect(pixels.byteCount).order(ByteOrder.nativeOrder()).also { uploadBuffer = it }
        buffer.clear()
        pixels.copyPixelsToBuffer(buffer)
        buffer.position(0)
        val texture = if (existing != 0) existing else newTexture(GLES30.GL_LINEAR, GLES30.GL_CLAMP_TO_EDGE)
        GLES30.glBindTexture(GLES30.GL_TEXTURE_2D, texture)
        GLES30.glPixelStorei(GLES30.GL_UNPACK_ALIGNMENT, 1)
        GLES30.glPixelStorei(GLES30.GL_UNPACK_ROW_LENGTH, pixels.rowBytes / BYTES_PER_PIXEL)
        GLES30.glTexImage2D(
            GLES30.GL_TEXTURE_2D, 0, GLES30.GL_RGBA8, pixels.width, pixels.height, 0,
            GLES30.GL_RGBA, GLES30.GL_UNSIGNED_BYTE, buffer,
        )
        GLES30.glPixelStorei(GLES30.GL_UNPACK_ROW_LENGTH, 0)
        return texture
    }

    private fun newTexture(filter: Int, wrap: Int): Int {
        val texture = IntArray(1).also { GLES30.glGenTextures(1, it, 0) }[0]
        GLES30.glBindTexture(GLES30.GL_TEXTURE_2D, texture)
        GLES30.glTexParameteri(GLES30.GL_TEXTURE_2D, GLES30.GL_TEXTURE_MIN_FILTER, filter)
        GLES30.glTexParameteri(GLES30.GL_TEXTURE_2D, GLES30.GL_TEXTURE_MAG_FILTER, filter)
        GLES30.glTexParameteri(GLES30.GL_TEXTURE_2D, GLES30.GL_TEXTURE_WRAP_S, wrap)
        GLES30.glTexParameteri(GLES30.GL_TEXTURE_2D, GLES30.GL_TEXTURE_WRAP_T, wrap)
        return texture
    }

    private fun quad(): Int {
        if (quad != 0) return quad
        quad = IntArray(1).also { GLES30.glGenBuffers(1, it, 0) }[0]
        val corners = floatArrayOf(-1f, -1f, 1f, -1f, -1f, 1f, 1f, 1f)
        val data = ByteBuffer.allocateDirect(corners.size * Float.SIZE_BYTES).order(ByteOrder.nativeOrder())
            .asFloatBuffer().put(corners).position(0)
        GLES30.glBindBuffer(GLES30.GL_ARRAY_BUFFER, quad)
        GLES30.glBufferData(GLES30.GL_ARRAY_BUFFER, corners.size * Float.SIZE_BYTES, data, GLES30.GL_STATIC_DRAW)
        return quad
    }

    private fun target(index: Int): FloatTarget {
        val slot = index % targets.size
        targets[slot]?.takeIf { it.width == width && it.height == height }?.let { return it }
        targets[slot]?.delete()
        return FloatTarget(width, height).also { targets[slot] = it }
    }

    private fun deleteGlObjects() {
        val textures = listOf(baseTexture, noiseTexture) + imageTextures.values
        textures.filter { it != 0 }.forEach { GLES30.glDeleteTextures(1, intArrayOf(it), 0) }
        if (quad != 0) GLES30.glDeleteBuffers(1, intArrayOf(quad), 0)
        targets.forEach { it?.delete() }
    }

    private fun fail(material: ByteArray?, reason: MaterialFailure) {
        if (broken) return
        broken = true
        onFailure(material, reason)
    }

    private companion object {
        const val QUAD_VERTICES = 4
        const val NOISE_SIDE = 1024
        const val BYTES_PER_PIXEL = 4
        const val MAX_ARGUMENT_FLOATS = 4
        const val NO_TEXTURE = 0
    }
}
