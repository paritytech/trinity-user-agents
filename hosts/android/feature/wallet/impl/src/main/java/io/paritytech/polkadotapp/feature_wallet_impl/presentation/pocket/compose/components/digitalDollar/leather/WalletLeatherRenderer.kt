package io.paritytech.polkadotapp.feature_wallet_impl.presentation.pocket.compose.components.digitalDollar.leather

import android.content.res.AssetManager
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.ColorSpace
import android.opengl.GLES30
import android.opengl.GLUtils
import java.nio.ByteBuffer

/**
 * The leather wallet card: the reference's baked photograph lit as leather, thread and brass by a light that
 * follows the phone (`paritytech/wallet-card-native`, whose README explains every term). Its `wallet.vert` and
 * `wallet.frag` are shipped unchanged; the drop shadow is not drawn, because the card sits in a stack with the
 * identity card peeking under it. The quad fills the surface, which is sized to the card.
 *
 * Every instance method runs on the thread holding the GL context.
 */
internal class WalletLeatherRenderer(private val assets: AssetManager) {
    private var program = 0
    private var vertexArray = 0
    private val textures = IntArray(2)
    private val uniforms = HashMap<String, Int>()

    fun init() {
        program = link(text("wallet.vert"), text("wallet.frag"))
        vertexArray = IntArray(1).also { GLES30.glGenVertexArrays(1, it, 0) }[0]
        GLES30.glGenTextures(2, textures, 0)

        val image = picture(assets)
        bind(unit = 0, filter = GLES30.GL_LINEAR)
        GLUtils.texImage2D(GLES30.GL_TEXTURE_2D, 0, image, 0)
        image.recycle()

        uploadRegions()
    }

    /** [tiltX], [tiltY] are the shader's tilt: `rest + amplitude * hand`. */
    fun draw(widthPx: Int, heightPx: Int, tiltX: Float, tiltY: Float) {
        GLES30.glViewport(0, 0, widthPx, heightPx)
        GLES30.glClearColor(0f, 0f, 0f, 0f)
        GLES30.glClear(GLES30.GL_COLOR_BUFFER_BIT)
        GLES30.glEnable(GLES30.GL_BLEND)
        GLES30.glBlendFunc(GLES30.GL_ONE, GLES30.GL_ONE_MINUS_SRC_ALPHA)
        GLES30.glBindVertexArray(vertexArray)
        GLES30.glUseProgram(program)

        GLES30.glUniform4f(uniform("uRect"), -1f, 1f, 1f, -1f)
        texture(name = "uImg", unit = 0)
        texture(name = "uReg", unit = 1)
        GLES30.glUniform2f(uniform("uTex"), IMAGE_WIDTH.toFloat(), IMAGE_HEIGHT.toFloat())
        GLES30.glUniform2f(uniform("uTilt"), tiltX, tiltY)
        GLES30.glUniform1f(uniform("uCrease"), CREASES)
        GLES30.glUniform1f(uniform("uGuard"), GUARD)
        GLES30.glUniform1f(uniform("uOcclusion"), OCCLUSION)
        GLES30.glUniform1fv(uniform("uMetal"), MATERIAL_SLOTS, Materials.metal, 0)
        GLES30.glUniform1fv(uniform("uRough"), MATERIAL_SLOTS, Materials.rough, 0)
        GLES30.glUniform1fv(uniform("uRelief"), MATERIAL_SLOTS, Materials.relief, 0)
        GLES30.glUniform1fv(uniform("uSheen"), MATERIAL_SLOTS, Materials.sheen, 0)
        GLES30.glUniform1fv(uniform("uCoat"), MATERIAL_SLOTS, Materials.coat, 0)
        GLES30.glDrawArrays(GLES30.GL_TRIANGLE_STRIP, 0, 4)
    }

    fun release() {
        uniforms.clear()
        GLES30.glDeleteTextures(2, textures, 0)
        GLES30.glDeleteVertexArrays(1, intArrayOf(vertexArray), 0)
        GLES30.glDeleteProgram(program)
    }

    private fun uniform(name: String) = uniforms.getOrPut(name) { GLES30.glGetUniformLocation(program, name) }

    private fun texture(name: String, unit: Int) {
        GLES30.glActiveTexture(GLES30.GL_TEXTURE0 + unit)
        GLES30.glBindTexture(GLES30.GL_TEXTURE_2D, textures[unit])
        GLES30.glUniform1i(uniform(name), unit)
    }

    private fun bind(unit: Int, filter: Int) {
        GLES30.glActiveTexture(GLES30.GL_TEXTURE0 + unit)
        GLES30.glBindTexture(GLES30.GL_TEXTURE_2D, textures[unit])
        GLES30.glTexParameteri(GLES30.GL_TEXTURE_2D, GLES30.GL_TEXTURE_MIN_FILTER, filter)
        GLES30.glTexParameteri(GLES30.GL_TEXTURE_2D, GLES30.GL_TEXTURE_MAG_FILTER, filter)
        GLES30.glTexParameteri(GLES30.GL_TEXTURE_2D, GLES30.GL_TEXTURE_WRAP_S, GLES30.GL_CLAMP_TO_EDGE)
        GLES30.glTexParameteri(GLES30.GL_TEXTURE_2D, GLES30.GL_TEXTURE_WRAP_T, GLES30.GL_CLAMP_TO_EDGE)
    }

    /** An id map, so its bytes go up as data: no colour conversion, no premultiplying, nearest sampling. */
    private fun uploadRegions() {
        val map = assets.bitmap(REGIONS, premultiplied = false)
        val rgba = ByteBuffer.allocateDirect(map.byteCount)
        map.copyPixelsToBuffer(rgba)

        val red = ByteBuffer.allocateDirect(map.width * map.height)
        for (pixel in 0 until map.width * map.height) red.put(rgba.get(pixel * 4))
        red.position(0)

        bind(unit = 1, filter = GLES30.GL_NEAREST)
        GLES30.glPixelStorei(GLES30.GL_UNPACK_ALIGNMENT, 1)
        GLES30.glTexImage2D(
            GLES30.GL_TEXTURE_2D, 0, GLES30.GL_R8, map.width, map.height, 0,
            GLES30.GL_RED, GLES30.GL_UNSIGNED_BYTE, red
        )
        GLES30.glPixelStorei(GLES30.GL_UNPACK_ALIGNMENT, 4)
        map.recycle()
    }

    private fun text(name: String) = assets.open("$FOLDER/$name").bufferedReader().use { it.readText() }

    private fun link(vertex: String, fragment: String): Int {
        val linked = GLES30.glCreateProgram()

        GLES30.glAttachShader(linked, compile(GLES30.GL_VERTEX_SHADER, vertex))
        GLES30.glAttachShader(linked, compile(GLES30.GL_FRAGMENT_SHADER, fragment))
        GLES30.glLinkProgram(linked)

        val status = IntArray(1)
        GLES30.glGetProgramiv(linked, GLES30.GL_LINK_STATUS, status, 0)
        check(status[0] != 0) { "wallet program did not link: ${GLES30.glGetProgramInfoLog(linked)}" }

        return linked
    }

    private fun compile(type: Int, source: String): Int {
        val shader = GLES30.glCreateShader(type)

        GLES30.glShaderSource(shader, source)
        GLES30.glCompileShader(shader)

        val status = IntArray(1)
        GLES30.glGetShaderiv(shader, GLES30.GL_COMPILE_STATUS, status, 0)
        check(status[0] != 0) { "wallet shader did not compile: ${GLES30.glGetShaderInfoLog(shader)}" }

        return shader
    }

    /** The reference's `card.json` materials, by region id: leather, thread, brass. Zero-filled to six. */
    private object Materials {
        val metal = slots(0.16f, 0f, 1f)
        val rough = slots(0.57f, 0.88f, 0.34f)
        val relief = slots(0.45f, 0.7f, 0.45f)
        val sheen = slots(0.1f, 0.5f, 0f)
        val coat = slots(0.22f, 0f, 0f)

        private fun slots(vararg values: Float) = FloatArray(MATERIAL_SLOTS).also { values.copyInto(it) }
    }

    companion object {
        const val IMAGE_WIDTH = 1024
        const val IMAGE_HEIGHT = 642
        const val ASPECT = IMAGE_WIDTH.toFloat() / IMAGE_HEIGHT

        private const val FOLDER = "wallet"
        private const val IMAGE = "image.png"
        private const val REGIONS = "regions.png"
        private const val MATERIAL_SLOTS = 6

        /** Crease-scale relief and the head-on guard, both on, as the reference's `params.json` ships. */
        private const val CREASES = 1f
        private const val GUARD = 1f

        /** The photo's own shadows take no highlight, as the reference's `params.json` ships. */
        private const val OCCLUSION = 1f

        fun picture(assets: AssetManager): Bitmap = assets.bitmap(IMAGE, premultiplied = true)

        private fun AssetManager.bitmap(name: String, premultiplied: Boolean): Bitmap = open("$FOLDER/$name").use { stream ->
            val options = BitmapFactory.Options().apply {
                inPreferredConfig = Bitmap.Config.ARGB_8888
                inPremultiplied = premultiplied
                inScaled = false
                inPreferredColorSpace = ColorSpace.get(ColorSpace.Named.SRGB)
            }

            checkNotNull(BitmapFactory.decodeStream(stream, null, options)) { "cannot decode $FOLDER/$name" }
        }
    }
}
