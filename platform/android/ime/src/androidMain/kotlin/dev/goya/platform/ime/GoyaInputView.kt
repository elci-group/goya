package dev.goya.platform.ime

import android.content.Context
import android.util.AttributeSet
import android.view.View
import android.view.inputmethod.EditorInfo
import android.view.inputmethod.InputConnection
import android.view.inputmethod.InputMethodManager

/**
 * Minimal Android view that exposes a Goya-owned text field to the platform
 * IME. Rendering is supplied by the embedding host; this view only owns the
 * Android input-connection lifecycle.
 */
class GoyaInputView @JvmOverloads constructor(
    context: Context,
    attrs: AttributeSet? = null,
    defStyleAttr: Int = 0,
) : View(context, attrs, defStyleAttr) {
    private var adapter: GoyaImeAdapter? = null
    private var bridge: GoyaInputBridge? = null
    private var cursorAnchorPublisher: GoyaCursorAnchorPublisher? = null
    private var focusChanged: ((Boolean) -> Unit)? = null
    private var editorAction: ((Int) -> Boolean)? = null
    private var singleLine: Boolean = true
    private var requestedImeAction: Int = EditorInfo.IME_ACTION_DONE
    private var activeConnection: GoyaInputConnection? = null

    init {
        isFocusable = true
        isFocusableInTouchMode = true
    }

    /** Bind this view to one already-focused runtime input node. */
    fun bindInput(
        adapter: GoyaImeAdapter,
        bridge: GoyaInputBridge,
        singleLine: Boolean = true,
        imeAction: Int = EditorInfo.IME_ACTION_DONE,
        onFocusChanged: ((Boolean) -> Unit)? = null,
        onEditorAction: ((Int) -> Boolean)? = null,
        cursorAnchorPublisher: GoyaCursorAnchorPublisher? = null,
    ) {
        activeConnection?.invalidate()
        this.adapter = adapter
        this.bridge = bridge
        this.singleLine = singleLine
        this.requestedImeAction = imeAction
        this.focusChanged = onFocusChanged
        this.editorAction = onEditorAction
        this.cursorAnchorPublisher = cursorAnchorPublisher
        activeConnection = null
    }

    override fun onCheckIsTextEditor(): Boolean = adapter != null && bridge != null

    override fun onCreateInputConnection(outAttrs: EditorInfo): InputConnection? {
        val currentAdapter = adapter ?: return null
        val currentBridge = bridge ?: return null
        activeConnection?.invalidate()
        val action = if (editorAction == null) {
            EditorInfo.IME_ACTION_UNSPECIFIED
        } else {
            requestedImeAction
        }
        GoyaInputConnection.configureEditorInfo(
            outAttrs = outAttrs,
            multiline = !singleLine,
            imeAction = action,
        )
        return GoyaInputConnection(
            targetView = this,
            adapter = currentAdapter,
            bridge = currentBridge,
            cursorAnchorPublisher = cursorAnchorPublisher,
            onEditorAction = editorAction,
            singleLine = singleLine,
        ).also { activeConnection = it }
    }

    override fun onFocusChanged(
        focused: Boolean,
        direction: Int,
        previouslyFocusedRect: android.graphics.Rect?,
    ) {
        super.onFocusChanged(focused, direction, previouslyFocusedRect)
        focusChanged?.invoke(focused)
    }

    override fun onDetachedFromWindow() {
        activeConnection?.invalidate()
        activeConnection = null
        super.onDetachedFromWindow()
    }

    /** Request focus and ask Android to display the currently selected IME. */
    fun showKeyboard(): Boolean {
        if (!isAttachedToWindow || !onCheckIsTextEditor()) return false
        requestFocus()
        val manager = context.getSystemService(Context.INPUT_METHOD_SERVICE) as? InputMethodManager
            ?: return false
        return manager.showSoftInput(this, InputMethodManager.SHOW_IMPLICIT)
    }

    /** Hide the IME associated with this view's current window token. */
    fun hideKeyboard(): Boolean {
        if (!isAttachedToWindow) return false
        val manager = context.getSystemService(Context.INPUT_METHOD_SERVICE) as? InputMethodManager
            ?: return false
        return manager.hideSoftInputFromWindow(windowToken, 0)
    }
}
