package dev.goya.platform.clipboard

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class GoyaClipboardTest {
    @Test fun emptyCopyAndCutNeverWrite() {
        for (operation in listOf(ClipboardOperation.COPY, ClipboardOperation.CUT)) {
            var writes = 0
            val result = writeText(operation, "", { null }) { writes++; ClipboardResult.Success(Unit) }
            assertEquals(ClipboardResult.NoChange, result)
            assertEquals(0, writes)
        }
    }

    @Test fun accessIsRequiredAndReadsAreUnsupported() {
        val denied = ClipboardResult.Failure(ClipboardFailure.DENIED)
        assertEquals(denied, writeText(ClipboardOperation.COPY, "", { denied }) { error("native write") })
        assertEquals(denied, readText { denied })
        assertEquals(ClipboardResult.Failure(ClipboardFailure.UNSUPPORTED), readText { null })
    }

    @Test fun validatesUnicodeAndUtf8ByteBound() {
        assertNull(validateText(""))
        assertNull(validateText("a\uD83D\uDE00\r\n"))
        assertEquals(ClipboardFailure.INVALID_TEXT, validateText("\uD800"))
        assertEquals(ClipboardFailure.INVALID_TEXT, validateText("\uDC00"))
        assertNull(validateText("é".repeat(524288)))
        assertEquals(ClipboardFailure.TOO_LARGE, validateText("é".repeat(524289)))
        assertEquals(ClipboardFailure.TOO_LARGE, validateText("a".repeat(1048573) + "😀"))
    }
}
