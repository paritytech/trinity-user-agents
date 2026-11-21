package io.paritytech.polkadotapp.feature_products_impl.presentation.materials

import android.graphics.Bitmap
import android.graphics.SurfaceTexture
import android.opengl.EGL14
import android.opengl.EGLConfig
import android.opengl.EGLContext
import android.opengl.EGLDisplay
import android.opengl.EGLExt
import android.opengl.EGLSurface
import android.opengl.GLES30
import android.os.SystemClock
import timber.log.Timber
import uniffi.truapi.ImageRef
import uniffi.truapi.MaterialFailure
import uniffi.truapi.MaterialShader
import uniffi.truapi.Parameter
import uniffi.truapi.ReadyLayer
import uniffi.truapi.TextureRole
import uniffi.truapi.UniformKind
import uniffi.truapi.materialNoise
import java.nio.ByteBuffer
import java.nio.ByteOrder

class MaterialDrawFailure(val material: ByteArray?, val reason: MaterialFailure)

class MaterialsGlRenderer(
    private val surface: SurfaceTexture,
    private val layers: List<ReadyLayer>,
    private val noiseSeed: UInt,
    private val motion: () -> DeviceMotion?,
    private val stilled: Boolean,
    private val onFailure: (MaterialDrawFailure) -> Unit,
) : Thread("MaterialsGlRenderer") {
    private val lock = Object()
    private var running = true
    private var surfaceHandedOver = false
    private var ended = false
    private var pendingBase: Bitmap? = null
    private var width = 0
    private var height = 0

    private var display: EGLDisplay = EGL14.EGL_NO_DISPLAY
    private var eglContext: EGLContext = EGL14.EGL_NO_CONTEXT
    private var eglSurface: EGLSurface = EGL14.EGL_NO_SURFACE

    private val animates = !stilled && layers.any { it.shader.readsTime || it.shader.readsMotion }

    fun updateBase(base: Bitmap, width: Int, height: Int) = synchronized(lock) {
        pendingBase = base
        this.width = width
        this.height = height
        lock.notifyAll()
    }

    fun finish() = synchronized(lock) {
        running = false
        lock.notifyAll()
    }

    // False when the thread has already ended, so the view releases the surface itself.
    fun handOverSurface(): Boolean = synchronized(lock) {
        running = false
        surfaceHandedOver = true
        lock.notifyAll()
        !ended
    }

    override fun run() {
        try {
            draw()
        } catch (cause: Exception) {
            Timber.w(cause, "Materials: the renderer stopped, the node stays plain")
            reportUnsupported(layers.firstOrNull()?.material)
        } finally {
            release()
        }
    }

    private fun draw() {
        if (!initialiseEgl()) {
            Timber.w("Materials: EGL unavailable, the node stays plain")
            reportUnsupported(layers.firstOrNull()?.material)
            return
        }
        val unloadedImage = layers.firstOrNull { layer -> layer.args.any { it.namesImage() } }
        if (unloadedImage != null) {
            reportUnsupported(unloadedImage.material)
            return
        }
        val programs = layers.map { layer -> compile(layer) }
        val unlinked = programs.indexOf(0)
        if (unlinked >= 0) {
            reportUnsupported(layers[unlinked].material)
            return
        }
        val format = LayerTargetFormat.of(GLES30.glGetString(GLES30.GL_EXTENSIONS).orEmpty())
        val noise = uploadNoise()
        val startedAt = SystemClock.elapsedRealtime()
        var base = 0
        var targets: List<LayerTarget> = emptyList()
        try {
            while (true) {
                val newBase = synchronized(lock) {
                    while (running && pendingBase == null && (!animates || base == 0)) lock.wait()
                    pendingBase.also { pendingBase = null }
                }
                if (!synchronized(lock) { running }) break
                if (newBase != null) {
                    if (base != 0) GLES30.glDeleteTextures(1, intArrayOf(base), 0)
                    base = uploadBase(newBase)
                    targets.forEach { it.delete() }
                    targets = List(minOf(PING_PONG_TARGETS, layers.size - 1)) { LayerTarget.create(width, height, format) }
                    if (targets.any { it.framebuffer == 0 }) {
                        reportUnsupported(layers.first().material)
                        break
                    }
                }
                val seconds = (SystemClock.elapsedRealtime() - startedAt) / MILLIS_PER_SECOND
                drawFrame(programs, base, noise, targets, format, frameInputs(seconds, motion(), stilled, width, height))
                EGL14.eglSwapBuffers(display, eglSurface)
            }
        } finally {
            targets.forEach { it.delete() }
        }
    }

    private fun reportUnsupported(material: ByteArray?) = onFailure(MaterialDrawFailure(material, MaterialFailure.UNSUPPORTED))

    private fun drawFrame(programs: List<Int>, base: Int, noise: Int, targets: List<LayerTarget>, format: LayerTargetFormat, inputs: FrameInputs) {
        var input = base
        layers.forEachIndexed { index, layer ->
            val last = index == layers.lastIndex
            val target = if (last) null else targets[index % targets.size]
            GLES30.glBindFramebuffer(GLES30.GL_FRAMEBUFFER, target?.framebuffer ?: 0)
            GLES30.glViewport(0, 0, width, height)
            GLES30.glClearColor(0f, 0f, 0f, 0f)
            GLES30.glClear(GLES30.GL_COLOR_BUFFER_BIT)
            if (last) {
                GLES30.glEnable(GLES30.GL_BLEND)
                // The shader writes straight alpha and the view composites premultiplied.
                GLES30.glBlendFuncSeparate(GLES30.GL_SRC_ALPHA, GLES30.GL_ONE_MINUS_SRC_ALPHA, GLES30.GL_ONE, GLES30.GL_ONE_MINUS_SRC_ALPHA)
            } else {
                GLES30.glDisable(GLES30.GL_BLEND)
            }
            val unfilterableSlots = if (index > 0 && !format.filterable) layer.shader.inputSlots() else emptySet()
            val program = programs[index]
            GLES30.glUseProgram(program)
            setUniforms(program, materialUniforms(layer.shader, layer.args, inputs, if (last) 1f else -1f, unfilterableSlots))
            layer.shader.textures.forEachIndexed { slot, texture ->
                val name = when (val role = texture.role) {
                    TextureRole.Base -> base
                    TextureRole.Input -> input
                    TextureRole.Noise -> noise
                    is TextureRole.Argument -> layer.args.getOrNull(role.v1.toInt()).argumentTexture(noise)
                }
                GLES30.glActiveTexture(GLES30.GL_TEXTURE0 + slot)
                GLES30.glBindTexture(GLES30.GL_TEXTURE_2D, name)
                GLES30.glUniform1i(GLES30.glGetUniformLocation(program, texture.name), slot)
            }
            drawQuad(program)
            input = target?.texture ?: input
        }
    }

    private fun MaterialShader.inputSlots(): Set<Int> =
        textures.indices.filter { slot -> textures[slot].role is TextureRole.Input }.toSet()

    private fun Parameter?.argumentTexture(noise: Int): Int = if (this?.imageRef() is ImageRef.Noise) noise else 0

    // This host loads no image arguments yet, so a layer that names one draws plain and is reported.
    private fun Parameter.namesImage(): Boolean = imageRef() is ImageRef.Source

    private fun Parameter.imageRef(): ImageRef? = when (this) {
        is Parameter.SamplerFloat -> v1
        is Parameter.SamplerVec2 -> v1
        is Parameter.SamplerVec3 -> v1
        is Parameter.SamplerVec4 -> v1
        is Parameter.Float, is Parameter.Vec2, is Parameter.Vec3, is Parameter.Vec4 -> null
    }

    private fun setUniforms(program: Int, values: List<MaterialUniformValue>) {
        values.forEach { value ->
            val location = GLES30.glGetUniformLocation(program, value.name)
            when (value) {
                is MaterialUniformValue.Integer -> GLES30.glUniform1i(location, value.value)
                is MaterialUniformValue.Floats -> setFloats(location, value.kind, value.values.toFloatArray())
            }
        }
    }

    private fun setFloats(location: Int, kind: UniformKind, values: FloatArray) {
        when (kind) {
            UniformKind.FLOAT -> GLES30.glUniform1fv(location, 1, values, 0)
            UniformKind.VEC2 -> GLES30.glUniform2fv(location, 1, values, 0)
            UniformKind.VEC3 -> GLES30.glUniform3fv(location, 1, values, 0)
            UniformKind.VEC4 -> GLES30.glUniform4fv(location, 1, values, 0)
            UniformKind.MAT3 -> GLES30.glUniformMatrix3fv(location, 1, false, values, 0)
            UniformKind.INT -> GLES30.glUniform1i(location, values.first().toInt())
        }
    }

    private fun drawQuad(program: Int) {
        val position = GLES30.glGetAttribLocation(program, "position")
        GLES30.glEnableVertexAttribArray(position)
        GLES30.glVertexAttribPointer(position, 2, GLES30.GL_FLOAT, false, 0, QUAD)
        GLES30.glDrawArrays(GLES30.GL_TRIANGLE_STRIP, 0, 4)
        GLES30.glDisableVertexAttribArray(position)
    }

    private fun compile(layer: ReadyLayer): Int {
        val vertex = compileShader(GLES30.GL_VERTEX_SHADER, layer.shader.vertex)
        val fragment = compileShader(GLES30.GL_FRAGMENT_SHADER, layer.shader.fragment)
        if (vertex == 0 || fragment == 0) return 0
        val program = GLES30.glCreateProgram()
        GLES30.glAttachShader(program, vertex)
        GLES30.glAttachShader(program, fragment)
        GLES30.glLinkProgram(program)
        GLES30.glDeleteShader(vertex)
        GLES30.glDeleteShader(fragment)
        val linked = IntArray(1)
        GLES30.glGetProgramiv(program, GLES30.GL_LINK_STATUS, linked, 0)
        if (linked[0] == 0) {
            Timber.w("Materials: link failed: %s", GLES30.glGetProgramInfoLog(program))
            GLES30.glDeleteProgram(program)
            return 0
        }
        return program
    }

    private fun compileShader(type: Int, source: String): Int {
        val shader = GLES30.glCreateShader(type)
        GLES30.glShaderSource(shader, source)
        GLES30.glCompileShader(shader)
        val compiled = IntArray(1)
        GLES30.glGetShaderiv(shader, GLES30.GL_COMPILE_STATUS, compiled, 0)
        if (compiled[0] == 0) {
            Timber.w("Materials: compile failed: %s", GLES30.glGetShaderInfoLog(shader))
            GLES30.glDeleteShader(shader)
            return 0
        }
        return shader
    }

    private fun uploadNoise(): Int {
        val pixels = materialNoise(noiseSeed)
        val texture = createTexture(GLES30.GL_NEAREST, GLES30.GL_REPEAT)
        GLES30.glPixelStorei(GLES30.GL_UNPACK_ALIGNMENT, 1)
        GLES30.glTexImage2D(
            GLES30.GL_TEXTURE_2D, 0, GLES30.GL_R8, NOISE_SIDE, NOISE_SIDE, 0,
            GLES30.GL_RED, GLES30.GL_UNSIGNED_BYTE, ByteBuffer.wrap(pixels),
        )
        return texture
    }

    // Uploaded from unpremultiplied pixels, top row first, as the format reads images.
    private fun uploadBase(bitmap: Bitmap): Int {
        val argb = IntArray(bitmap.width * bitmap.height)
        bitmap.getPixels(argb, 0, bitmap.width, 0, 0, bitmap.width, bitmap.height)
        val rgba = ByteBuffer.allocateDirect(argb.size * BYTES_PER_PIXEL).order(ByteOrder.nativeOrder())
        argb.forEach { pixel ->
            rgba.put((pixel shr 16).toByte()).put((pixel shr 8).toByte()).put(pixel.toByte()).put((pixel ushr 24).toByte())
        }
        rgba.position(0)
        val texture = createTexture(GLES30.GL_LINEAR, GLES30.GL_CLAMP_TO_EDGE)
        GLES30.glPixelStorei(GLES30.GL_UNPACK_ALIGNMENT, BYTES_PER_PIXEL)
        GLES30.glTexImage2D(
            GLES30.GL_TEXTURE_2D, 0, GLES30.GL_RGBA8, bitmap.width, bitmap.height, 0,
            GLES30.GL_RGBA, GLES30.GL_UNSIGNED_BYTE, rgba,
        )
        return texture
    }

    // 32-bit floats keep the precision the format asks for between layers. A GPU that cannot filter
    // them samples them nearest and the shader filters, and 8 bits is the last resort.
    private enum class LayerTargetFormat(val internalFormat: Int, val type: Int, val filterable: Boolean) {
        FLOAT_FILTERED(GLES30.GL_RGBA32F, GLES30.GL_FLOAT, filterable = true),
        FLOAT_UNFILTERED(GLES30.GL_RGBA32F, GLES30.GL_FLOAT, filterable = false),
        BYTE(GLES30.GL_RGBA8, GLES30.GL_UNSIGNED_BYTE, filterable = true),
        ;

        companion object {
            fun of(extensions: String): LayerTargetFormat = when {
                "GL_EXT_color_buffer_float" !in extensions -> BYTE
                "GL_OES_texture_float_linear" in extensions -> FLOAT_FILTERED
                else -> FLOAT_UNFILTERED
            }
        }
    }

    private class LayerTarget(val framebuffer: Int, val texture: Int) {
        fun delete() {
            GLES30.glDeleteFramebuffers(1, intArrayOf(framebuffer), 0)
            GLES30.glDeleteTextures(1, intArrayOf(texture), 0)
        }

        companion object {
            fun create(width: Int, height: Int, format: LayerTargetFormat): LayerTarget {
                val texture = createTexture(if (format.filterable) GLES30.GL_LINEAR else GLES30.GL_NEAREST, GLES30.GL_CLAMP_TO_EDGE)
                GLES30.glTexImage2D(GLES30.GL_TEXTURE_2D, 0, format.internalFormat, width, height, 0, GLES30.GL_RGBA, format.type, null)
                val framebuffer = IntArray(1)
                GLES30.glGenFramebuffers(1, framebuffer, 0)
                GLES30.glBindFramebuffer(GLES30.GL_FRAMEBUFFER, framebuffer[0])
                GLES30.glFramebufferTexture2D(GLES30.GL_FRAMEBUFFER, GLES30.GL_COLOR_ATTACHMENT0, GLES30.GL_TEXTURE_2D, texture, 0)
                val complete = GLES30.glCheckFramebufferStatus(GLES30.GL_FRAMEBUFFER) == GLES30.GL_FRAMEBUFFER_COMPLETE
                GLES30.glBindFramebuffer(GLES30.GL_FRAMEBUFFER, 0)
                return LayerTarget(if (complete) framebuffer[0] else 0, texture)
            }
        }
    }

    private fun initialiseEgl(): Boolean {
        display = EGL14.eglGetDisplay(EGL14.EGL_DEFAULT_DISPLAY)
        if (display == EGL14.EGL_NO_DISPLAY) return false
        val version = IntArray(2)
        if (!EGL14.eglInitialize(display, version, 0, version, 1)) return false
        val config = chooseConfig() ?: return false
        eglContext = EGL14.eglCreateContext(
            display, config, EGL14.EGL_NO_CONTEXT, intArrayOf(EGL14.EGL_CONTEXT_CLIENT_VERSION, GLES_VERSION, EGL14.EGL_NONE), 0,
        )
        if (eglContext == EGL14.EGL_NO_CONTEXT) return false
        eglSurface = EGL14.eglCreateWindowSurface(display, config, surface, intArrayOf(EGL14.EGL_NONE), 0)
        if (eglSurface == EGL14.EGL_NO_SURFACE) return false
        return EGL14.eglMakeCurrent(display, eglSurface, eglSurface, eglContext)
    }

    private fun chooseConfig(): EGLConfig? {
        val attributes = intArrayOf(
            EGL14.EGL_RENDERABLE_TYPE, EGLExt.EGL_OPENGL_ES3_BIT_KHR,
            EGL14.EGL_SURFACE_TYPE, EGL14.EGL_WINDOW_BIT,
            EGL14.EGL_RED_SIZE, 8,
            EGL14.EGL_GREEN_SIZE, 8,
            EGL14.EGL_BLUE_SIZE, 8,
            EGL14.EGL_ALPHA_SIZE, 8,
            EGL14.EGL_NONE,
        )
        val configs = arrayOfNulls<EGLConfig>(1)
        val count = IntArray(1)
        val chosen = EGL14.eglChooseConfig(display, attributes, 0, configs, 0, 1, count, 0)
        return if (chosen && count[0] > 0) configs[0] else null
    }

    // A surface the view handed over is released here, after the last frame, never while a swap
    // may still be drawing into it. One the view still holds is the view's to release.
    private fun release() {
        if (display != EGL14.EGL_NO_DISPLAY) {
            EGL14.eglMakeCurrent(display, EGL14.EGL_NO_SURFACE, EGL14.EGL_NO_SURFACE, EGL14.EGL_NO_CONTEXT)
            if (eglSurface != EGL14.EGL_NO_SURFACE) EGL14.eglDestroySurface(display, eglSurface)
            if (eglContext != EGL14.EGL_NO_CONTEXT) EGL14.eglDestroyContext(display, eglContext)
            EGL14.eglTerminate(display)
        }
        val handedOver = synchronized(lock) {
            ended = true
            surfaceHandedOver
        }
        if (handedOver) surface.release()
    }

    private companion object {
        const val NOISE_SIDE = 1024
        const val MILLIS_PER_SECOND = 1000f
        const val PING_PONG_TARGETS = 2
        const val BYTES_PER_PIXEL = 4
        const val GLES_VERSION = 3

        // Counter-clockwise from the bottom left, so the quad faces front on a bottom-up target.
        val QUAD = ByteBuffer.allocateDirect(8 * Float.SIZE_BYTES).order(ByteOrder.nativeOrder()).asFloatBuffer()
            .put(floatArrayOf(-1f, -1f, 1f, -1f, -1f, 1f, 1f, 1f)).also { it.position(0) }

        fun createTexture(filter: Int, wrap: Int): Int {
            val texture = IntArray(1)
            GLES30.glGenTextures(1, texture, 0)
            GLES30.glBindTexture(GLES30.GL_TEXTURE_2D, texture[0])
            GLES30.glTexParameteri(GLES30.GL_TEXTURE_2D, GLES30.GL_TEXTURE_MIN_FILTER, filter)
            GLES30.glTexParameteri(GLES30.GL_TEXTURE_2D, GLES30.GL_TEXTURE_MAG_FILTER, filter)
            GLES30.glTexParameteri(GLES30.GL_TEXTURE_2D, GLES30.GL_TEXTURE_WRAP_S, wrap)
            GLES30.glTexParameteri(GLES30.GL_TEXTURE_2D, GLES30.GL_TEXTURE_WRAP_T, wrap)
            return texture[0]
        }
    }
}
