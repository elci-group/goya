package dev.goya.platform.ime

import java.nio.charset.StandardCharsets

/** A read-only point-in-time copy of the Rust TextBuffer state. */
data class EditorSnapshot(
    val text: String,
    val anchorUtf8: Int,
    val cursorUtf8: Int,
)

/** Preedit cursor offsets and delete lengths use UTF-8 bytes, matching GY-RUNTIME/1. */
data class ImeBatch(
    val preedit: Preedit? = null,
    val commit: String? = null,
    val deleteBeforeUtf8: Int? = null,
    val deleteAfterUtf8: Int? = null,
)

data class Preedit(val text: String, val cursorBeginUtf8: Int, val cursorEndUtf8: Int)

/**
 * Narrow JNI boundary used by an Android InputConnection wrapper.
 *
 * The Rust runtime remains the source of truth for text, selection, undo and
 * grapheme behavior. Snapshots are read-only and must be obtained afresh for
 * each operation. `setSelectionUtf8` and cursor-anchor requests are explicit
 * because GY-RUNTIME/1 currently has no absolute-selection or cursor-anchor
 * Event variant; an Android host must route those to its platform bridge.
 */
interface GoyaInputBridge {
    fun editorSnapshot(): EditorSnapshot?
    fun applyImeBatch(batch: ImeBatch): Boolean
    fun setSelectionUtf8(anchor: Int, cursor: Int): Boolean
    fun requestCursorAnchorUpdates(mode: Int, filter: Int): Boolean
}

/**
 * Converts Android InputConnection's UTF-16 offsets/counts to the UTF-8 byte
 * offsets used by Goya. This class does not retain or edit a second text model.
 */
class GoyaImeAdapter(private val bridge: GoyaInputBridge) {
    fun setComposingText(text: CharSequence, newCursorPosition: Int): Boolean {
        val value = text.toString()
        val cursor = composingCursorUtf16(value, newCursorPosition)
        val cursorByte = byteOffsetAtUtf16(value, cursor)
        return bridge.applyImeBatch(
            ImeBatch(preedit = Preedit(value, cursorByte, cursorByte)),
        )
    }

    fun commitText(text: CharSequence, newCursorPosition: Int): Boolean {
        val snapshot = bridge.editorSnapshot() ?: return false
        val textValue = text.toString()
        val anchor = clampUtf8Boundary(snapshot.text, snapshot.anchorUtf8)
        val cursor = clampUtf8Boundary(snapshot.text, snapshot.cursorUtf8)
        val replaceStart = minOf(anchor, cursor)
        val replaceEnd = maxOf(anchor, cursor)
        val prefix = snapshot.text.substring(0, utf16OffsetAtByte(snapshot.text, replaceStart))
        val suffix = snapshot.text.substring(utf16OffsetAtByte(snapshot.text, replaceEnd))
        val resultingText = prefix + textValue + suffix
        val insertionStartUtf16 = prefix.length
        val insertionEndUtf16 = insertionStartUtf16 + textValue.length
        val targetUtf16 = if (newCursorPosition > 0) {
            insertionEndUtf16 + newCursorPosition - 1
        } else {
            insertionStartUtf16 + newCursorPosition
        }
        val targetByte = byteOffsetAtUtf16(resultingText, targetUtf16)

        if (!bridge.applyImeBatch(ImeBatch(commit = textValue))) return false
        return bridge.setSelectionUtf8(targetByte, targetByte)
    }

    /** Android lengths are UTF-16 code units; Goya deletion lengths are UTF-8 bytes. */
    fun deleteSurroundingText(beforeLength: Int, afterLength: Int): Boolean {
        if (beforeLength < 0 || afterLength < 0) return false
        val snapshot = bridge.editorSnapshot() ?: return false
        val cursorByte = clampUtf8Boundary(snapshot.text, snapshot.cursorUtf8)
        val cursorUtf16 = utf16OffsetAtByte(snapshot.text, cursorByte)
        val beforeUtf16 = safeUtf16Boundary(snapshot.text, cursorUtf16 - beforeLength)
        val afterUtf16 = safeUtf16Boundary(snapshot.text, cursorUtf16 + afterLength)
        val beforeBytes = cursorByte - byteOffsetAtUtf16(snapshot.text, beforeUtf16)
        val afterBytes = byteOffsetAtUtf16(snapshot.text, afterUtf16) - cursorByte
        return bridge.applyImeBatch(
            ImeBatch(deleteBeforeUtf8 = beforeBytes, deleteAfterUtf8 = afterBytes),
        )
    }

    /** Absolute Android selection indices are UTF-16; Goya stores byte offsets. */
    fun setSelection(start: Int, end: Int): Boolean {
        val snapshot = bridge.editorSnapshot() ?: return false
        val anchor = byteOffsetAtUtf16(snapshot.text, start)
        val cursor = byteOffsetAtUtf16(snapshot.text, end)
        return bridge.setSelectionUtf8(anchor, cursor)
    }

    /** Called from InputConnection.requestCursorUpdates; geometry comes from the host's latest render. */
    fun requestCursorUpdates(mode: Int, filter: Int): Boolean =
        bridge.requestCursorAnchorUpdates(mode, filter)

    private fun composingCursorUtf16(text: String, newCursorPosition: Int): Int {
        val target = if (newCursorPosition > 0) {
            text.length + newCursorPosition - 1
        } else {
            text.length + newCursorPosition
        }
        return safeUtf16Boundary(text, target)
    }
}

private fun byteOffsetAtUtf16(text: String, requested: Int): Int {
    val index = safeUtf16Boundary(text, requested)
    return text.substring(0, index).toByteArray(StandardCharsets.UTF_8).size
}

private fun utf16OffsetAtByte(text: String, requested: Int): Int {
    val target = requested.coerceIn(0, text.toByteArray(StandardCharsets.UTF_8).size)
    var byteOffset = 0
    var utf16Offset = 0
    while (utf16Offset < text.length) {
        val codePoint = text.codePointAt(utf16Offset)
        val width = String(Character.toChars(codePoint)).toByteArray(StandardCharsets.UTF_8).size
        if (byteOffset + width > target) break
        byteOffset += width
        utf16Offset += Character.charCount(codePoint)
    }
    return utf16Offset
}

private fun safeUtf16Boundary(text: String, requested: Int): Int {
    val index = requested.coerceIn(0, text.length)
    return if (
        index > 0 && index < text.length &&
        Character.isHighSurrogate(text[index - 1]) && Character.isLowSurrogate(text[index])
    ) index - 1 else index
}

private fun clampUtf8Boundary(text: String, requested: Int): Int =
    byteOffsetAtUtf16(text, utf16OffsetAtByte(text, requested))
