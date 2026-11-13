package dev.goya.platform.clipboard

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class GoyaClipboardTest {
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
