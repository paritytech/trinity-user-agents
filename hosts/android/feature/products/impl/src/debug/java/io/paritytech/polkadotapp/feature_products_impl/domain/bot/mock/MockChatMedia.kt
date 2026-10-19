package io.paritytech.polkadotapp.feature_products_impl.domain.bot.mock

import android.content.Context
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Color
import android.graphics.LinearGradient
import android.graphics.Paint
import android.graphics.Shader
import android.graphics.pdf.PdfDocument
import dagger.hilt.android.qualifiers.ApplicationContext
import io.paritytech.polkadotapp.feature_products_impl.R
import java.io.ByteArrayOutputStream
import javax.inject.Inject
import javax.inject.Singleton

/**
 * The bytes of the pictures, icons, avatars, file and video of the mock product. The content takes them as bytes, so it
 * builds without Android graphics.
 */
class MockChatMedia(
    val pictures: List<ByteArray>,
    val video: ByteArray?,
    val document: ByteArray,
    val icon: (Glyph) -> ByteArray,
    val avatar: (Glyph, Int) -> ByteArray,
) {
    /** A drawn sign, not a letter, so it cannot be taken for the letter avatar that the host draws when an image fails. */
    enum class Glyph(val text: String) {
        PHONE("☎"),
        SPARKLES("✦"),
        FOOTER("▤"),
        LINK("↗"),
        STAR("★"),
        HEART("♥"),
        HELP("?"),
        STACK("❖"),
        TRAY("▣"),
        PIN("⚲"),
        THREAD("❝"),
        PARTY("✺"),
        TRASH("✕"),
        REFRESH("↻"),
        AT("@"),
        MUTE("∅"),
        INFO("ℹ"),
    }

    companion object {
        /** A BlurHash that the host draws before the bytes of an item load. */
        const val PLACEHOLDER = "LEHV6nWB2yk8pyo0adR*.7kCMdnj"
        const val PICTURE_WIDTH = 640
        const val PICTURE_HEIGHT = 400
        const val VIDEO_SIDE = 240
    }
}

@Singleton
class MockChatMediaFactory @Inject constructor(
    @param:ApplicationContext private val context: Context,
) {
    fun make(): MockChatMedia {
        val gradients = listOf(
            Color.rgb(255, 45, 85) to Color.rgb(175, 82, 222),
            Color.rgb(48, 176, 199) to Color.rgb(0, 122, 255),
            Color.rgb(255, 149, 0) to Color.rgb(255, 59, 48),
            Color.rgb(52, 199, 89) to Color.rgb(0, 199, 190),
        )
        return MockChatMedia(
            pictures = gradients.mapIndexed { index, colors -> picture("Image ${index + 1}", colors) },
            video = runCatching { context.resources.openRawResource(R.raw.mock_chat_video).use { it.readBytes() } }.getOrNull(),
            document = document(),
            icon = ::icon,
            avatar = ::avatar,
        )
    }

    private fun picture(title: String, colors: Pair<Int, Int>): ByteArray {
        val width = MockChatMedia.PICTURE_WIDTH.toFloat()
        val height = MockChatMedia.PICTURE_HEIGHT.toFloat()
        return draw(MockChatMedia.PICTURE_WIDTH, MockChatMedia.PICTURE_HEIGHT, Bitmap.CompressFormat.JPEG) { canvas ->
            val paint = Paint().apply { shader = LinearGradient(0f, 0f, width, height, colors.first, colors.second, Shader.TileMode.CLAMP) }
            canvas.drawRect(0f, 0f, width, height, paint)
            canvas.drawCentered(title, width, height, size = height / 6)
        }
    }

    private fun icon(glyph: MockChatMedia.Glyph): ByteArray = draw(ICON_SIDE, ICON_SIDE, Bitmap.CompressFormat.PNG) { canvas ->
        canvas.drawCentered(glyph.text, ICON_SIDE.toFloat(), ICON_SIDE.toFloat(), size = ICON_SIDE * 0.7f)
    }

    private fun avatar(glyph: MockChatMedia.Glyph, color: Int): ByteArray = draw(AVATAR_SIDE, AVATAR_SIDE, Bitmap.CompressFormat.PNG) { canvas ->
        val side = AVATAR_SIDE.toFloat()
        canvas.drawCircle(side / 2, side / 2, side / 2, Paint(Paint.ANTI_ALIAS_FLAG).apply { this.color = color })
        canvas.drawCentered(glyph.text, side, side, size = side * 0.5f)
    }

    private fun document(): ByteArray {
        val pdf = PdfDocument()
        val page = pdf.startPage(PdfDocument.PageInfo.Builder(PDF_WIDTH, PDF_HEIGHT, 1).create())
        val paint = Paint(Paint.ANTI_ALIAS_FLAG).apply { textSize = 18f }
        listOf(
            "Chat Showcase",
            "",
            "This file is a File media item of a RichText body.",
            "The host opens it, and the user can share it from there.",
        ).forEachIndexed { line, text -> page.canvas.drawText(text, 48f, 80f + line * 28f, paint) }
        pdf.finishPage(page)
        return ByteArrayOutputStream().use { output ->
            pdf.writeTo(output)
            pdf.close()
            output.toByteArray()
        }
    }

    private fun draw(width: Int, height: Int, format: Bitmap.CompressFormat, paint: (Canvas) -> Unit): ByteArray {
        val bitmap = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888)
        paint(Canvas(bitmap))
        return ByteArrayOutputStream().use { output ->
            bitmap.compress(format, JPEG_QUALITY, output)
            bitmap.recycle()
            output.toByteArray()
        }
    }

    private fun Canvas.drawCentered(text: String, width: Float, height: Float, size: Float) {
        val paint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
            color = Color.WHITE
            textSize = size
            textAlign = Paint.Align.CENTER
            isFakeBoldText = true
        }
        val baseline = height / 2 - (paint.descent() + paint.ascent()) / 2
        drawText(text, width / 2, baseline, paint)
    }

    private companion object {
        const val ICON_SIDE = 96
        const val AVATAR_SIDE = 288
        const val PDF_WIDTH = 595
        const val PDF_HEIGHT = 842
        const val JPEG_QUALITY = 70
    }
}
