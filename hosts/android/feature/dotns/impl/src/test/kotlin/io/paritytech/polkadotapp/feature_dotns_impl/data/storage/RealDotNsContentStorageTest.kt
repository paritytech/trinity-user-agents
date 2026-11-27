package io.paritytech.polkadotapp.feature_dotns_impl.data.storage

import android.content.Context
import io.mockk.every
import io.mockk.mockk
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.File

class RealDotNsContentStorageTest {
    @get:Rule
    val filesDir = TemporaryFolder()

    private val context: Context = mockk {
        every { filesDir } answers { this@RealDotNsContentStorageTest.filesDir.root }
    }
    private val storage = RealDotNsContentStorage(context)

    @Test
    fun `an archive is not on the device until it is fully unpacked`() = runTest {
        var directoryWhileUnpacking: File? = null

        storage.saveContentStreaming(CONTENT_HASH) { writer ->
            writer.write("/index.html", "<html></html>".byteInputStream())
            directoryWhileUnpacking = storage.getContentDirectory(CONTENT_HASH)
            writer.write("/app.js", "start()".byteInputStream())
        }

        assertNull(directoryWhileUnpacking)
        assertEquals("start()", File(storage.getContentDirectory(CONTENT_HASH), "app.js").readText())
    }

    private companion object {
        const val CONTENT_HASH = "0123abcd"
    }
}
