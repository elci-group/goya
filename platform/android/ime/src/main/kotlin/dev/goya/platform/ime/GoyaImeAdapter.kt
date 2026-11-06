package dev.goya.platform.ime

import java.nio.charset.StandardCharsets

/** A read-only point-in-time copy of the Rust TextBuffer state. */
data class EditorSnapshot(
    val text: String,
    val anchorUtf8: Int,
    val cursorUtf8: Int,
    val composingStartUtf8: Int = -1,
    val composingEndUtf8: Int = -1,
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
 * grapheme behavior. Implementations dispatch synchronously against the
 * focused node: `setComposingRegionUtf8` maps to SetCompositionRegion,
 * `commitInputText` to CommitInputText, `deleteInputSurrounding` to
 * DeleteInputSurrounding, and `finishComposition` to FinishComposition.
 * `setSelectionUtf8` maps to SetInputSelection. Cursor-anchor requests go to
 * the Android host, which publishes geometry from its latest render.
 *
 * The Android host must call adapter methods on the runtime's single
 * serialized editor thread. In particular, snapshot, UTF-16 conversion and
 * DeleteInputSurrounding dispatch must not be interleaved with another edit;
 * the host must hold its editor serialization across the whole adapter call.
 * Commit avoids this snapshot/mutation race by using one atomic runtime event.
 */
interface GoyaInputBridge {
    /** Synchronous read from the runtime-owned editor state. */
    fun editorSnapshot(): EditorSnapshot?
    fun applyImeBatch(batch: ImeBatch): Boolean
    fun setSelectionUtf8(anchor: Int, cursor: Int): Boolean
    /** Mark an existing text range composing without moving selection or changing text. */
    fun setComposingRegionUtf8(start: Int, end: Int): Boolean
    /** Atomically replace composition/selection and place the caret in committed text. */
    fun commitInputText(text: String, cursorByteInText: Int): Boolean
    /** Delete around both selection endpoints without deleting the selection. */
    fun deleteInputSurrounding(beforeBytes: Int, afterBytes: Int): Boolean
    fun finishComposition(): Boolean
    fun requestCursorAnchorUpdates(mode: Int, filter: Int): Boolean
}

/**
 * Converts Android InputConnection's UTF-16 offsets/counts to the UTF-8 byte
 * offsets used by Goya. This class does not retain or edit a second text model.
 * Calls that read a snapshot and then mutate must run synchronously on the
 * same serialized editor executor; otherwise another edit could invalidate
 * the byte ranges between those calls. Commit is one atomic bridge operation.
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
        val textValue = text.toString()
        val targetUtf16 = if (newCursorPosition > 0) {
            textValue.length + newCursorPosition - 1
        } else {
            textValue.length + newCursorPosition
        }
        val cursorByteInText = byteOffsetAtUtf16(textValue, targetUtf16)
        return bridge.commitInputText(textValue, cursorByteInText)
    }

    /** Android lengths are UTF-16 code units; Goya deletion lengths are UTF-8 bytes. */
    fun deleteSurroundingText(beforeLength: Int, afterLength: Int): Boolean {
        if (beforeLength < 0 || afterLength < 0) return false
        val snapshot = bridge.editorSnapshot() ?: return false
        val anchorByte = clampUtf8Boundary(snapshot.text, snapshot.anchorUtf8)
        val cursorByte = clampUtf8Boundary(snapshot.text, snapshot.cursorUtf8)
        val selectionStartByte = minOf(anchorByte, cursorByte)
        val selectionEndByte = maxOf(anchorByte, cursorByte)
        val selectionStartUtf16 = utf16OffsetAtByte(snapshot.text, selectionStartByte)
        val selectionEndUtf16 = utf16OffsetAtByte(snapshot.text, selectionEndByte)
        val beforeUtf16 = safeUtf16Boundary(snapshot.text, selectionStartUtf16 - beforeLength)
        val afterUtf16 = safeUtf16Boundary(snapshot.text, selectionEndUtf16 + afterLength)
        val beforeBytes = selectionStartByte - byteOffsetAtUtf16(snapshot.text, beforeUtf16)
        val afterBytes = byteOffsetAtUtf16(snapshot.text, afterUtf16) - selectionEndByte
        return bridge.deleteInputSurrounding(beforeBytes, afterBytes)
    }

    /** Absolute Android selection indices are UTF-16; Goya stores byte offsets. */
    fun setSelection(start: Int, end: Int): Boolean {
        val snapshot = bridge.editorSnapshot() ?: return false
        val anchor = byteOffsetAtUtf16(snapshot.text, start)
        val cursor = byteOffsetAtUtf16(snapshot.text, end)
        return bridge.setSelectionUtf8(anchor, cursor)
    }

    /** Mark an existing range as composing; reversed endpoints are equivalent. */
    fun setComposingRegion(start: Int, end: Int): Boolean {
        val snapshot = bridge.editorSnapshot() ?: return false
        val first = byteOffsetAtUtf16(snapshot.text, start)
        val second = byteOffsetAtUtf16(snapshot.text, end)
        val regionStart = minOf(first, second)
        val regionEnd = maxOf(first, second)
        if (regionStart == regionEnd) return finishComposingText()
        return bridge.setComposingRegionUtf8(regionStart, regionEnd)
    }

    /** Finish an active preedit through GY-RUNTIME/1 Event::FinishComposition. */
    fun finishComposingText(): Boolean = bridge.finishComposition()

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
