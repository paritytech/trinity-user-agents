package io.paritytech.polkadotapp.feature_products_api.presentation.widget.materials

import android.content.Context
import android.opengl.EGL14
import android.opengl.EGLConfig
import android.opengl.EGLContext
import android.opengl.EGLDisplay
import android.opengl.EGLExt
import android.opengl.EGLSurface
import android.opengl.GLES30
import android.os.Handler
import android.os.HandlerThread
import android.os.Looper
import android.os.PowerManager
import android.os.SystemClock
import android.view.Surface
import dagger.hilt.android.qualifiers.ApplicationContext
import io.novasama.substrate_sdk_android.extensions.toHexString
import io.paritytech.polkadotapp.common.utils.logFailure
import io.paritytech.polkadotapp.common.utils.sha256
import kotlinx.coroutines.flow.MutableStateFlow
import timber.log.Timber
import uniffi.truapi.MaterialShader
import java.io.DataInputStream
import java.io.DataOutputStream
import java.io.File
import java.nio.ByteBuffer
import java.nio.ByteOrder
import javax.inject.Inject
import javax.inject.Singleton
import kotlin.time.Duration.Companion.milliseconds

/**
 * The one GL thread and OpenGL ES 3.0 context every materials surface draws on, so a material compiles once
 * however many surfaces show it. The thread starts when the first surface needs it.
 */
@Singleton
class MaterialsGl @Inject constructor(@ApplicationContext private val appContext: Context) {
    /** Records each compile on disk, so a material that took the app down is not compiled again. */
    val guard: CompileGuard by lazy {
        CompileGuard(appContext.getSharedPreferences(GUARD_PREFS, Context.MODE_PRIVATE))
    }

    /** True while the device is hot or saving power, when every node draws plain. */
    val allPlain = MutableStateFlow(false)

    internal val handler: Handler by lazy {
        Handler(HandlerThread("materials-gl").apply { start() }.looper).also { gl ->
            gl.post {
                ready = runCatching { setUp() }
                    .logFailure("materials: no GL ES 3.0 context, every node draws plain")
                    .isSuccess
            }
        }
    }

    private var display: EGLDisplay = EGL14.EGL_NO_DISPLAY
    private var config: EGLConfig? = null
    private var eglContext: EGLContext = EGL14.EGL_NO_CONTEXT
    private var pbuffer: EGLSurface = EGL14.EGL_NO_SURFACE
    private var ready = false
    private var programs: GlPrograms? = null

    internal var floatTargets = false
        private set

    private var attached = 0
    private val stepDown = StepDown()
    private val watchStrain = Runnable { watchStrain() }

    internal val applicationContext: Context get() = appContext

    internal fun program(material: ByteArray, shader: MaterialShader): Result<Int> {
        if (Looper.myLooper() != handler.looper) {
            return Result.failure(IllegalStateException("materials: GL called off its thread"))
        }
        val linker = programs ?: return Result.failure(IllegalStateException("materials: no GL context"))
        return linker.program(material, shader)
    }

    internal fun windowSurface(surface: Surface): EGLSurface? {
        if (!ready) return null
        val created = EGL14.eglCreateWindowSurface(display, config, surface, intArrayOf(EGL14.EGL_NONE), 0)
        return created.takeIf { it != EGL14.EGL_NO_SURFACE }
    }

    internal fun makeCurrent(surface: EGLSurface?): Boolean =
        ready && EGL14.eglMakeCurrent(display, surface ?: pbuffer, surface ?: pbuffer, eglContext)

    // One context serves every surface, so an error another surface left must not fail this one.
    internal fun clearErrors() {
        repeat(ERROR_FLAGS) { if (GLES30.glGetError() == GLES30.GL_NO_ERROR) return }
    }

    internal fun swap(surface: EGLSurface): Boolean = EGL14.eglSwapBuffers(display, surface)

    internal fun destroy(surface: EGLSurface) {
        if (EGL14.eglGetCurrentSurface(EGL14.EGL_DRAW) == surface) makeCurrent(null)
        EGL14.eglDestroySurface(display, surface)
    }

    internal fun surfaceAttached() {
        attached++
        handler.removeCallbacks(watchStrain)
        watchStrain()
    }

    internal fun surfaceDetached() {
        attached--
    }

    // While every node is plain no surface is attached, yet the calm period still has to be watched.
    private fun watchStrain() {
        val power = appContext.getSystemService(PowerManager::class.java)
        val strained = power != null &&
            (power.isPowerSaveMode || power.currentThermalStatus >= PowerManager.THERMAL_STATUS_SEVERE)
        allPlain.value = stepDown.update(SystemClock.elapsedRealtime().milliseconds, strained)
        if (attached > 0 || allPlain.value) handler.postDelayed(watchStrain, STRAIN_POLL_MILLIS)
    }

    private fun setUp() {
        display = EGL14.eglGetDisplay(EGL14.EGL_DEFAULT_DISPLAY)
        val version = IntArray(2)
        check(EGL14.eglInitialize(display, version, 0, version, 1)) { "eglInitialize failed" }

        val attributes = intArrayOf(
            EGL14.EGL_RENDERABLE_TYPE, EGLExt.EGL_OPENGL_ES3_BIT_KHR,
            EGL14.EGL_SURFACE_TYPE, EGL14.EGL_WINDOW_BIT or EGL14.EGL_PBUFFER_BIT,
            EGL14.EGL_RED_SIZE, 8,
            EGL14.EGL_GREEN_SIZE, 8,
            EGL14.EGL_BLUE_SIZE, 8,
            EGL14.EGL_ALPHA_SIZE, 8,
            EGL14.EGL_NONE,
        )
        val configs = arrayOfNulls<EGLConfig>(1)
        val count = IntArray(1)
        check(EGL14.eglChooseConfig(display, attributes, 0, configs, 0, 1, count, 0) && count[0] > 0) {
            "no RGBA8 ES3 config"
        }
        config = configs[0]
        eglContext = EGL14.eglCreateContext(
            display,
            config,
            EGL14.EGL_NO_CONTEXT,
            intArrayOf(EGL14.EGL_CONTEXT_CLIENT_VERSION, 3, EGL14.EGL_NONE),
            0,
        )
        check(eglContext != EGL14.EGL_NO_CONTEXT) { "eglCreateContext failed" }
        pbuffer = EGL14.eglCreatePbufferSurface(
            display,
            config,
            intArrayOf(EGL14.EGL_WIDTH, 1, EGL14.EGL_HEIGHT, 1, EGL14.EGL_NONE),
            0,
        )
        check(EGL14.eglMakeCurrent(display, pbuffer, pbuffer, eglContext)) { "eglMakeCurrent failed" }

        floatTargets = GLES30.glGetString(GLES30.GL_EXTENSIONS).orEmpty()
            .split(' ').contains("GL_EXT_color_buffer_float")
        val formats = IntArray(1)
        GLES30.glGetIntegerv(GLES30.GL_NUM_PROGRAM_BINARY_FORMATS, formats, 0)
        val device = "${GLES30.glGetString(GLES30.GL_RENDERER)}|${GLES30.glGetString(GLES30.GL_VERSION)}"
        val directory = appContext.codeCacheDir.resolve("materials")
        programs = GlPrograms(directory, guard, device, keepsBinaries = formats[0] > 0)
    }

    private companion object {
        const val GUARD_PREFS = "materials_compile_guard"
        const val STRAIN_POLL_MILLIS = 1_000L

        // GL keeps one flag per kind of error, and ES 3.0 has six kinds.
        const val ERROR_FLAGS = 6
    }
}

// GL thread only, with a context current.
internal class GlPrograms(
    private val directory: File,
    private val guard: CompileGuard,
    private val device: String,
    private val keepsBinaries: Boolean,
) {
    private val linked = object : LinkedHashMap<Pair<String, String>, Int>(CAPACITY, LOAD_FACTOR, true) {
        override fun removeEldestEntry(eldest: MutableMap.MutableEntry<Pair<String, String>, Int>): Boolean {
            if (size <= CAPACITY) return false
            GLES30.glDeleteProgram(eldest.value)
            return true
        }
    }

    fun program(material: ByteArray, shader: MaterialShader): Result<Int> {
        val key = shader.vertex to shader.fragment
        linked[key]?.let { return Result.success(it) }
        if (guard.crashed(material)) {
            val reason = "materials: ${material.toHexString()} crashed the app compiling"
            return Result.failure(IllegalStateException(reason))
        }

        val started = SystemClock.elapsedRealtime()
        val file = directory.resolve("$device|${shader.vertex}|${shader.fragment}".toByteArray().sha256().toHexString())
        val fromDisk = if (keepsBinaries) loadBinary(file) else null
        val program = fromDisk ?: guard.compiling(material) { compile(shader) }.getOrElse { return Result.failure(it) }
        if (fromDisk == null && keepsBinaries) runCatching { saveBinary(program, file) }
            .onFailure { Timber.w(it, "materials: program binary not kept") }

        val source = if (fromDisk != null) "from disk" else "compiled"
        val millis = SystemClock.elapsedRealtime() - started
        Timber.i("materials: program %s in %d ms (%s)", material.toHexString(), millis, source)
        linked[key] = program
        return Result.success(program)
    }

    private fun compile(shader: MaterialShader): Result<Int> = runCatching {
        val vertex = compileStage(GLES30.GL_VERTEX_SHADER, shader.vertex)
        val fragment = compileStage(GLES30.GL_FRAGMENT_SHADER, shader.fragment)
        val program = GLES30.glCreateProgram()
        if (keepsBinaries) {
            GLES30.glProgramParameteri(program, GLES30.GL_PROGRAM_BINARY_RETRIEVABLE_HINT, GLES30.GL_TRUE)
        }
        GLES30.glAttachShader(program, vertex)
        GLES30.glAttachShader(program, fragment)
        GLES30.glLinkProgram(program)
        GLES30.glDeleteShader(vertex)
        GLES30.glDeleteShader(fragment)
        if (!linkSucceeded(program)) {
            val log = GLES30.glGetProgramInfoLog(program)
            GLES30.glDeleteProgram(program)
            error("materials: link failed: $log")
        }
        program
    }

    private fun compileStage(type: Int, source: String): Int {
        val stage = GLES30.glCreateShader(type)
        GLES30.glShaderSource(stage, source)
        GLES30.glCompileShader(stage)
        val status = IntArray(1)
        GLES30.glGetShaderiv(stage, GLES30.GL_COMPILE_STATUS, status, 0)
        if (status[0] == 0) {
            val log = GLES30.glGetShaderInfoLog(stage)
            GLES30.glDeleteShader(stage)
            error("materials: compile failed: $log")
        }
        return stage
    }

    private fun loadBinary(file: File): Int? {
        if (!file.exists()) return null
        val program = runCatching {
            DataInputStream(file.inputStream().buffered()).use { input ->
                val format = input.readInt()
                val bytes = input.readBytes()
                val binary = ByteBuffer.allocateDirect(bytes.size).order(ByteOrder.nativeOrder()).put(bytes)
                binary.position(0)
                GLES30.glCreateProgram().also { GLES30.glProgramBinary(it, format, binary, bytes.size) }
            }
        }.getOrNull()
        if (program != null && linkSucceeded(program)) return program
        // A driver update or a corrupt file leaves a binary that no longer links.
        program?.let(GLES30::glDeleteProgram)
        file.delete()
        return null
    }

    private fun saveBinary(program: Int, file: File) {
        val length = IntArray(1)
        GLES30.glGetProgramiv(program, GLES30.GL_PROGRAM_BINARY_LENGTH, length, 0)
        if (length[0] <= 0) return
        val binary = ByteBuffer.allocateDirect(length[0]).order(ByteOrder.nativeOrder())
        val format = IntArray(1)
        GLES30.glGetProgramBinary(program, length[0], length, 0, format, 0, binary)
        val bytes = ByteArray(length[0]).also { binary.get(it) }
        directory.mkdirs()
        DataOutputStream(file.outputStream().buffered()).use { output ->
            output.writeInt(format[0])
            output.write(bytes)
        }
    }

    private fun linkSucceeded(program: Int): Boolean {
        val status = IntArray(1)
        GLES30.glGetProgramiv(program, GLES30.GL_LINK_STATUS, status, 0)
        return status[0] != 0
    }

    private companion object {
        // The core's own shader cache capacity.
        const val CAPACITY = 64
        const val LOAD_FACTOR = 0.75f
    }
}
