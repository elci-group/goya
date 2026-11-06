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
 * Synchronously runs work on the single thread that owns the Goya runtime.
 * Implementations run inline when already on that thread, otherwise marshal
 * the operation and wait for its result. They must not return before it runs.
 */
interface GoyaEditorDispatcher {
    fun <T> callSynchronously(operation: () -> T): T
}

fun interface GoyaCursorAnchorPublisher {
    /**
     * Thread-safe request which schedules actual cursor-anchor publication on
     * the View thread. Return true only when the host accepted that request.
     */
    fun request(cursorUpdateMode: Int, cursorUpdateFilter: Int): Boolean
}

/**
 * Android's editor-facing connection for Goya input nodes.
 *
 * This class owns no Editable and no committed text. Every read comes from a
 * fresh runtime-owned [EditorSnapshot], and every edit is forwarded through
 * [GoyaImeAdapter]. All reads and edits run synchronously through
 * [GoyaEditorDispatcher], so snapshots and the mutation using them share one
 * serialized runtime-owner operation. Snapshot text, selection offsets, and
 * composing offsets all refer to the same displayed text, including preedit.
 */
class GoyaInputConnection(
    private val targetView: View,
    private val adapter: GoyaImeAdapter,
    private val bridge: GoyaInputBridge,
    private val editorDispatcher: GoyaEditorDispatcher,
    private val cursorAnchorPublisher: GoyaCursorAnchorPublisher? = null,
    private val onEditorAction: ((Int) -> Boolean)? = null,
    private val singleLine: Boolean = true,
) : BaseInputConnection(targetView, false) {
    private var batchDepth = 0
    @Volatile
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
        return edit {
            if (beforeLength < 0 || afterLength < 0) {
                false
            } else {
                val snapshot = bridge.editorSnapshot()
                if (snapshot == null) {
                    false
                } else {
                    val anchor = utf16OffsetAtByte(snapshot.text, snapshot.anchorUtf8)
                    val cursor = utf16OffsetAtByte(snapshot.text, snapshot.cursorUtf8)
                    val selectionStart = minOf(anchor, cursor)
                    val selectionEnd = maxOf(anchor, cursor)
                    val beforeStart = offsetBeforeCodePoints(
                        snapshot.text,
                        selectionStart,
                        beforeLength,
                    )
                    val afterEnd = offsetAfterCodePoints(
                        snapshot.text,
                        selectionEnd,
                        afterLength,
                    )
                    if (beforeStart == null || afterEnd == null) {
                        false
                    } else {
                        adapter.deleteSurroundingText(
                            selectionStart - beforeStart,
                            afterEnd - selectionEnd,
                        )
                    }
                }
            }
        }
    }

    override fun setSelection(start: Int, end: Int): Boolean =
        edit { adapter.setSelection(start, end) }

    override fun requestCursorUpdates(cursorUpdateMode: Int, cursorUpdateFilter: Int): Boolean {
        if (closed) return false
        val publisher = cursorAnchorPublisher ?: return false
        val registered = editorDispatcher.callSynchronously {
            !closed && adapter.requestCursorUpdates(cursorUpdateMode, cursorUpdateFilter)
        }
        return registered && publisher.request(cursorUpdateMode, cursorUpdateFilter)
    }

    override fun performEditorAction(actionCode: Int): Boolean =
        !closed && editorDispatcher.callSynchronously {
            !closed && (onEditorAction?.invoke(actionCode) ?: false)
        }

    override fun closeConnection() {
        invalidate()
        super.closeConnection()
    }

    override fun getTextBeforeCursor(length: Int, flags: Int): CharSequence? {
        if (closed || length < 0) return null
        return editorDispatcher.callSynchronously {
            if (closed) {
                null
            } else {
                val snapshot = bridge.editorSnapshot()
                if (snapshot == null) {
                    null
                } else {
                    val cursor = utf16OffsetAtByte(snapshot.text, snapshot.cursorUtf8)
                    val start = safeUtf16Boundary(snapshot.text, cursor - length)
                    snapshot.text.substring(start, cursor)
                }
            }
        }
    }

    override fun getTextAfterCursor(length: Int, flags: Int): CharSequence? {
        if (closed || length < 0) return null
        return editorDispatcher.callSynchronously {
            if (closed) {
                null
            } else {
                val snapshot = bridge.editorSnapshot()
                if (snapshot == null) {
                    null
                } else {
                    val cursor = utf16OffsetAtByte(snapshot.text, snapshot.cursorUtf8)
                    val end = safeUtf16Boundary(snapshot.text, cursor + length)
                    snapshot.text.substring(cursor, end)
                }
            }
        }
    }

    override fun getSelectedText(flags: Int): CharSequence? {
        if (closed) return null
        return editorDispatcher.callSynchronously {
            if (closed) {
                null
            } else {
                val snapshot = bridge.editorSnapshot()
                if (snapshot == null) {
                    null
                } else {
                    val anchor = utf16OffsetAtByte(snapshot.text, snapshot.anchorUtf8)
                    val cursor = utf16OffsetAtByte(snapshot.text, snapshot.cursorUtf8)
                    val start = minOf(anchor, cursor)
                    val end = maxOf(anchor, cursor)
                    if (start == end) null else snapshot.text.substring(start, end)
                }
            }
        }
    }

    override fun getExtractedText(
        request: ExtractedTextRequest?,
        flags: Int,
    ): ExtractedText? {
        if (closed) return null
        return editorDispatcher.callSynchronously {
            if (closed) {
                null
            } else {
                val snapshot = bridge.editorSnapshot()
                if (snapshot == null) {
                    null
                } else {
                    ExtractedText().apply {
                        text = snapshot.text
                        startOffset = 0
                        partialStartOffset = -1
                        partialEndOffset = -1
                        selectionStart = utf16OffsetAtByte(snapshot.text, snapshot.anchorUtf8)
                        selectionEnd = utf16OffsetAtByte(snapshot.text, snapshot.cursorUtf8)
                        flags = (if (singleLine) ExtractedText.FLAG_SINGLE_LINE else 0) or
                            if (snapshot.anchorUtf8 != snapshot.cursorUtf8) {
                                ExtractedText.FLAG_SELECTING
                            } else {
                                0
                            }
                    }
                }
            }
        }
    }

    override fun getCursorCapsMode(reqModes: Int): Int {
        if (closed) return 0
        return editorDispatcher.callSynchronously {
            if (closed) {
                0
            } else {
                val snapshot = bridge.editorSnapshot()
                if (snapshot == null) {
                    0
                } else {
                    val cursor = utf16OffsetAtByte(snapshot.text, snapshot.cursorUtf8)
                    TextUtils.getCapsMode(snapshot.text, cursor, reqModes)
                }
            }
        }
    }

    /** Stop accepting edits after the Android view detaches this connection. */
    fun invalidate() {
        closed = true
        batchDepth = 0
    }

    private inline fun edit(operation: () -> Boolean): Boolean {
        if (closed) return false
        val accepted = editorDispatcher.callSynchronously {
            !closed && operation()
        }
        if (accepted && batchDepth == 0) publishSelection()
        return accepted
    }

    private fun publishSelection() {
        if (closed) return
        val update = editorDispatcher.callSynchronously {
            if (closed) {
                null
            } else {
                bridge.editorSnapshot()?.let { snapshot ->
                    SelectionUpdate(
                        anchor = utf16OffsetAtByte(snapshot.text, snapshot.anchorUtf8),
                        cursor = utf16OffsetAtByte(snapshot.text, snapshot.cursorUtf8),
                        composingStart = if (snapshot.composingStartUtf8 >= 0) {
                            utf16OffsetAtByte(snapshot.text, snapshot.composingStartUtf8)
                        } else {
                            -1
                        },
                        composingEnd = if (snapshot.composingEndUtf8 >= 0) {
                            utf16OffsetAtByte(snapshot.text, snapshot.composingEndUtf8)
                        } else {
                            -1
                        },
                    )
                }
            }
        } ?: return
        targetView.post {
            if (closed || !targetView.isAttachedToWindow) return@post
            val manager = targetView.context
                .getSystemService(Context.INPUT_METHOD_SERVICE) as? InputMethodManager
            manager?.updateSelection(
                targetView,
                update.anchor,
                update.cursor,
                update.composingStart,
                update.composingEnd,
            )
        }
    }

    private data class SelectionUpdate(
        val anchor: Int,
        val cursor: Int,
        val composingStart: Int,
        val composingEnd: Int,
    )

    companion object {
        /** Configure a plain text editor without creating an Android text buffer. */
        @JvmStatic
        fun configureEditorInfo(
            outAttrs: EditorInfo,
            multiline: Boolean = false,
            imeAction: Int = EditorInfo.IME_ACTION_UNSPECIFIED,
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
