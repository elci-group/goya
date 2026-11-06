package dev.goya.platform.ime

import android.content.Context
import android.text.InputType
import android.text.TextUtils
import android.view.View
import android.view.inputmethod.BaseInputConnection
import android.view.inputmethod.EditorInfo
import android.view.inputmethod.ExtractedText
import android.view.inputmethod.ExtractedTextRequest
import android.view.inputmethod.InputMethodManager

/**
 * Android's editor-facing connection for Goya input nodes.
 *
 * This class owns no Editable and no committed text. Every read comes from a
 * fresh Rust-owned [EditorSnapshot], and every edit is forwarded through
 * [GoyaImeAdapter]. Calls must remain serialized on the runtime's editor
 * thread for the duration of each method, as required by the bridge contract.
 */
class GoyaInputConnection(
    private val targetView: View,
    private val adapter: GoyaImeAdapter,
    private val bridge: GoyaInputBridge,
) : BaseInputConnection(targetView, false) {
    private var batchDepth = 0
    private var closed = false

    override fun beginBatchEdit(): Boolean {
        if (closed) return false
        batchDepth += 1
        return true
    }

    override fun endBatchEdit(): Boolean {
        if (closed || batchDepth == 0) return false
        batchDepth -= 1
        if (batchDepth == 0) publishSelection()
        return true
    }

    override fun commitText(text: CharSequence?, newCursorPosition: Int): Boolean =
        edit { adapter.commitText(text ?: "", newCursorPosition) }

    override fun setComposingText(text: CharSequence?, newCursorPosition: Int): Boolean =
        edit { adapter.setComposingText(text ?: "", newCursorPosition) }

    override fun setComposingRegion(start: Int, end: Int): Boolean =
        edit { adapter.setComposingRegion(start, end) }

    override fun finishComposingText(): Boolean = edit(adapter::finishComposingText)

    override fun deleteSurroundingText(beforeLength: Int, afterLength: Int): Boolean =
        edit { adapter.deleteSurroundingText(beforeLength, afterLength) }

    override fun deleteSurroundingTextInCodePoints(
        beforeLength: Int,
        afterLength: Int,
    ): Boolean {
        if (closed || beforeLength < 0 || afterLength < 0) return false
        val snapshot = bridge.editorSnapshot() ?: return false
        val anchor = utf16OffsetAtByte(snapshot.text, snapshot.anchorUtf8)
        val cursor = utf16OffsetAtByte(snapshot.text, snapshot.cursorUtf8)
        val selectionStart = minOf(anchor, cursor)
        val selectionEnd = maxOf(anchor, cursor)
        val beforeStart = offsetBeforeCodePoints(snapshot.text, selectionStart, beforeLength)
            ?: return false
        val afterEnd = offsetAfterCodePoints(snapshot.text, selectionEnd, afterLength)
            ?: return false
        return edit {
            adapter.deleteSurroundingText(
                selectionStart - beforeStart,
                afterEnd - selectionEnd,
            )
        }
    }

    override fun setSelection(start: Int, end: Int): Boolean =
        edit { adapter.setSelection(start, end) }

    override fun requestCursorUpdates(cursorUpdateMode: Int, cursorUpdateFilter: Int): Boolean =
        !closed && adapter.requestCursorUpdates(cursorUpdateMode, cursorUpdateFilter)

    override fun getTextBeforeCursor(length: Int, flags: Int): CharSequence? {
        if (closed || length < 0) return null
        val snapshot = bridge.editorSnapshot() ?: return null
        val cursor = utf16OffsetAtByte(snapshot.text, snapshot.cursorUtf8)
        val start = safeUtf16Boundary(snapshot.text, cursor - length)
        return snapshot.text.substring(start, cursor)
    }

    override fun getTextAfterCursor(length: Int, flags: Int): CharSequence? {
        if (closed || length < 0) return null
        val snapshot = bridge.editorSnapshot() ?: return null
        val cursor = utf16OffsetAtByte(snapshot.text, snapshot.cursorUtf8)
        val end = safeUtf16Boundary(snapshot.text, cursor + length)
        return snapshot.text.substring(cursor, end)
    }

    override fun getSelectedText(flags: Int): CharSequence? {
        if (closed) return null
        val snapshot = bridge.editorSnapshot() ?: return null
        val anchor = utf16OffsetAtByte(snapshot.text, snapshot.anchorUtf8)
        val cursor = utf16OffsetAtByte(snapshot.text, snapshot.cursorUtf8)
        val start = minOf(anchor, cursor)
        val end = maxOf(anchor, cursor)
        return if (start == end) null else snapshot.text.substring(start, end)
    }

    override fun getExtractedText(
        request: ExtractedTextRequest?,
        flags: Int,
    ): ExtractedText? {
        if (closed) return null
        val snapshot = bridge.editorSnapshot() ?: return null
        val result = ExtractedText()
        result.text = snapshot.text
        result.startOffset = 0
        result.partialStartOffset = -1
        result.partialEndOffset = -1
        result.selectionStart = utf16OffsetAtByte(snapshot.text, snapshot.anchorUtf8)
        result.selectionEnd = utf16OffsetAtByte(snapshot.text, snapshot.cursorUtf8)
        result.flags = if (snapshot.anchorUtf8 != snapshot.cursorUtf8) {
            ExtractedText.FLAG_SELECTING
        } else {
            0
        }
        return result
    }

    override fun getCursorCapsMode(reqModes: Int): Int {
        if (closed) return 0
        val snapshot = bridge.editorSnapshot() ?: return 0
        val cursor = utf16OffsetAtByte(snapshot.text, snapshot.cursorUtf8)
        return TextUtils.getCapsMode(snapshot.text, cursor, reqModes)
    }

    /** Stop accepting edits after the Android view detaches this connection. */
    fun invalidate() {
        closed = true
        batchDepth = 0
    }

    private inline fun edit(operation: () -> Boolean): Boolean {
        if (closed) return false
        val accepted = operation()
        if (accepted && batchDepth == 0) publishSelection()
        return accepted
    }

    private fun publishSelection() {
        val snapshot = bridge.editorSnapshot() ?: return
        val anchor = utf16OffsetAtByte(snapshot.text, snapshot.anchorUtf8)
        val cursor = utf16OffsetAtByte(snapshot.text, snapshot.cursorUtf8)
        val composingStart = if (snapshot.composingStartUtf8 >= 0) {
            utf16OffsetAtByte(snapshot.text, snapshot.composingStartUtf8)
        } else {
            -1
        }
        val composingEnd = if (snapshot.composingEndUtf8 >= 0) {
            utf16OffsetAtByte(snapshot.text, snapshot.composingEndUtf8)
        } else {
            -1
        }
        val manager = targetView.context
            .getSystemService(Context.INPUT_METHOD_SERVICE) as? InputMethodManager
        manager?.updateSelection(targetView, anchor, cursor, composingStart, composingEnd)
    }

    companion object {
        /** Configure a plain text editor without creating an Android text buffer. */
        @JvmStatic
        fun configureEditorInfo(
            outAttrs: EditorInfo,
            multiline: Boolean = false,
            imeAction: Int = EditorInfo.IME_ACTION_DONE,
        ) {
            outAttrs.inputType = InputType.TYPE_CLASS_TEXT or
                if (multiline) InputType.TYPE_TEXT_FLAG_MULTI_LINE else 0
            outAttrs.imeOptions = imeAction
        }
    }
}

private fun utf16OffsetAtByte(text: String, requested: Int): Int {
    val target = requested.coerceIn(0, text.toByteArray(Charsets.UTF_8).size)
    var byteOffset = 0
    var utf16Offset = 0
    while (utf16Offset < text.length) {
        val codePoint = text.codePointAt(utf16Offset)
        val byteWidth = String(Character.toChars(codePoint)).toByteArray(Charsets.UTF_8).size
        if (byteOffset + byteWidth > target) break
        byteOffset += byteWidth
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

private fun offsetBeforeCodePoints(text: String, offset: Int, count: Int): Int? {
    var position = offset
    var remaining = count
    while (remaining > 0 && position > 0) {
        val current = text[position - 1]
        when {
            Character.isLowSurrogate(current) -> {
                if (position < 2 || !Character.isHighSurrogate(text[position - 2])) return null
                position -= 2
            }
            Character.isHighSurrogate(current) -> return null
            else -> position -= 1
        }
        remaining -= 1
    }
    return position
}

private fun offsetAfterCodePoints(text: String, offset: Int, count: Int): Int? {
    var position = offset
    var remaining = count
    while (remaining > 0 && position < text.length) {
        val current = text[position]
        when {
            Character.isHighSurrogate(current) -> {
                if (position + 1 >= text.length ||
                    !Character.isLowSurrogate(text[position + 1])) return null
                position += 2
            }
            Character.isLowSurrogate(current) -> return null
            else -> position += 1
        }
        remaining -= 1
    }
    return position
}
