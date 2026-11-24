//! Kotlin source templates used by the Android scene emitter.

pub(super) const SPEC_HELPERS: &str = r#"
    private fun place(r: LongArray, i: Int, px: Long, py: Long, pw: Long, ph: Long, w0: Long, h0: Long, x0: Long, y0: Long, overflow: Boolean) {
        val w = maxOf(w0, 0L)
        val h = maxOf(h0, 0L)
        val right = satAdd(px, pw)
        val bottom = satAdd(py, ph)
        val x = if (overflow) maxOf(x0, px) else minOf(maxOf(x0, px), right)
        val y = if (overflow) maxOf(y0, py) else minOf(maxOf(y0, py), bottom)
        r[4 * i] = x
        r[4 * i + 1] = y
        r[4 * i + 2] = if (overflow) w else maxOf(minOf(w, satSub(right, x)), 0L)
        r[4 * i + 3] = if (overflow) h else maxOf(minOf(h, satSub(bottom, y)), 0L)
    }

    private fun prop(fraction: Double, parent: Long): Long = (fraction * parent.toDouble()).toLong()

    fun satAdd(a: Long, b: Long): Long {
        val s = a + b
        return if (((a xor s) and (b xor s)) < 0L) (if (a < 0L) Long.MIN_VALUE else Long.MAX_VALUE) else s
    }

    fun satSub(a: Long, b: Long): Long {
        val d = a - b
        return if (((a xor b) and (a xor d)) < 0L) (if (a < 0L) Long.MIN_VALUE else Long.MAX_VALUE) else d
    }

    fun isAncestor(ancestor: Int, cell: Int): Boolean {
        var p = PARENTS[cell]
        while (p >= 0) {
            if (p == ancestor) return true
            p = PARENTS[p]
        }
        return false
    }

    /** Maximum [x, y] scroll offsets in millipoints of scroll cell [i]. */
    fun scrollLimits(r: LongArray, i: Int): LongArray {
        if (!SCROLL[i]) return longArrayOf(0L, 0L)
        val viewRight = satAdd(r[4 * i], r[4 * i + 2])
        val viewBottom = satAdd(r[4 * i + 1], r[4 * i + 3])
        var right = viewRight
        var bottom = viewBottom
        for (j in 0 until CELL_COUNT) {
            if (isAncestor(i, j)) {
                right = maxOf(right, satAdd(r[4 * j], r[4 * j + 2]))
                bottom = maxOf(bottom, satAdd(r[4 * j + 1], r[4 * j + 3]))
            }
        }
        return longArrayOf(satSub(right, viewRight), satSub(bottom, viewBottom))
    }

    /** Resolved text size of cell [i] in millipoints, or -1. */
    fun textSizeMp(r: LongArray, i: Int): Long =
        if (TEXT_SIZE_P[i] < 0.0) -1L else (TEXT_SIZE_P[i] * r[4 * i + 3].toDouble()).toLong()

    /** Resolved stroke width of cell [i] in millipoints, or -1. */
    fun strokeWidthMp(r: LongArray, i: Int): Long =
        if (STROKE_WIDTH_P[i] < 0.0) -1L else (STROKE_WIDTH_P[i] * minOf(r[4 * i + 2], r[4 * i + 3]).toDouble()).toLong()

    /** handoff::convert_field for `int`; null is NotCanonicalInt. */
    fun convertInt(text: String): Long? {
        val digits = if (text.startsWith("-")) text.substring(1) else text
        if (digits.isEmpty() || text == "-0") return null
        for (c in digits) if (c < '0' || c > '9') return null
        if (digits.length > 1 && digits[0] == '0') return null
        return text.toLongOrNull()
    }

    /** handoff::convert_field for `text`; false is TextLimit. */
    fun textWithinLimit(text: String): Boolean = utf8Length(text) <= MAX_FIELD_TEXT_BYTES

    fun utf8Length(text: String): Int {
        var bytes = 0
        var i = 0
        while (i < text.length) {
            val c = text[i]
            if (c.isHighSurrogate() && i + 1 < text.length && text[i + 1].isLowSurrogate()) {
                bytes += 4
                i += 2
                continue
            }
            bytes += when {
                c.code < 0x80 -> 1
                c.code < 0x800 -> 2
                else -> 3
            }
            i += 1
        }
        return bytes
    }
"#;

pub(super) const VIEW_IMPORTS: &str = "import android.content.Context
import android.graphics.drawable.GradientDrawable
import android.os.Build
import android.os.Bundle
import android.text.InputType
import android.util.TypedValue
import android.view.Gravity
import android.view.MotionEvent
import android.view.View
import android.view.ViewConfiguration
import android.view.ViewGroup
import android.view.WindowInsets
import android.view.accessibility.AccessibilityNodeInfo
import android.widget.Button
import android.widget.EditText
import android.widget.TextView

";

pub(super) const VIEW_LAYOUT: &str = r#"
    override fun onApplyWindowInsets(insets: WindowInsets): WindowInsets {
        if (Build.VERSION.SDK_INT >= 30) {
            val bars = insets.getInsets(WindowInsets.Type.systemBars() or WindowInsets.Type.displayCutout())
            setPadding(bars.left, bars.top, bars.right, bars.bottom)
        } else {
            @Suppress("DEPRECATION")
            setPadding(insets.systemWindowInsetLeft, insets.systemWindowInsetTop, insets.systemWindowInsetRight, insets.systemWindowInsetBottom)
        }
        return insets
    }

    internal fun measureCell(cell: GoyaCell) {
        val i = cell.index
        cell.measure(exactly(edges[4 * i + 2] - edges[4 * i]), exactly(edges[4 * i + 3] - edges[4 * i + 1]))
    }

    internal fun layoutCell(cell: GoyaCell, originX: Int, originY: Int) {
        val i = cell.index
        cell.layout(edges[4 * i] - originX, edges[4 * i + 1] - originY, edges[4 * i + 2] - originX, edges[4 * i + 3] - originY)
    }

    internal fun left(i: Int): Int = edges[4 * i]
    internal fun top(i: Int): Int = edges[4 * i + 1]
    internal fun maxScrollX(i: Int): Int = maxOf(limits[2 * i], 0)
    internal fun maxScrollY(i: Int): Int = maxOf(limits[2 * i + 1], 0)

    private fun exactly(size: Int): Int = MeasureSpec.makeMeasureSpec(maxOf(size, 0), MeasureSpec.EXACTLY)

    /** Millipoints to pixels: each edge rounds half up on its own. */
    private fun px(mp: Long): Int =
"#;

pub(super) const VIEW_CONTENT: &str = r#"
    private fun label(text: String): TextView = TextView(context).apply {
        this.text = text
        setHorizontallyScrolling(true)
        includeFontPadding = false
        setPadding(0, 0, 0, 0)
        gravity = Gravity.TOP or Gravity.START
    }

    private fun button(text: String, onClick: (() -> Unit)?): Button = Button(context).apply {
        this.text = text
        isAllCaps = false
        minWidth = 0
        minHeight = 0
        minimumWidth = 0
        minimumHeight = 0
        setPadding(0, 0, 0, 0)
        if (onClick != null) setOnClickListener { onClick() }
    }

    private fun field(fieldId: String, viewId: Int, initialText: String): EditText = EditText(context).apply {
        id = viewId
        minWidth = 0
        minHeight = 0
        minimumWidth = 0
        minimumHeight = 0
        setPadding(0, 0, 0, 0)
        inputType = InputType.TYPE_CLASS_TEXT
        setText(initialText)
        fields[fieldId] = this
    }
"#;

pub(super) const VIEW_CELL: &str = r#"
/** One scene element. Its own content fills it; child cells use scene edges. */
class GoyaCell(context: Context, private val scene: GoyaSceneView, val index: Int) : ViewGroup(context) {
    private var content: View? = null
    private var layer: GoyaScrollLayer? = null
"#;

pub(super) const VIEW_CLASSES: &str = r#"
    fun setContent(view: View) {
        content = view
        addView(view, 0)
    }

    /**
     * `@iden` (GSSH/4, decision D14) through Android's native accessibility:
     * the label is the accessible name, the hint the accessible description
     * (the node's tooltip text from API 28; appended to the name before).
     * An editable field keeps speaking its own text: the label becomes its
     * hint text (API 26+) rather than a content description.
     */
    fun identify(label: String, hint: String?) {
        val target = content ?: this
        target.importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_YES
        target.accessibilityDelegate = object : View.AccessibilityDelegate() {
            override fun onInitializeAccessibilityNodeInfo(host: View, info: AccessibilityNodeInfo) {
                super.onInitializeAccessibilityNodeInfo(host, info)
                val tooltip = Build.VERSION.SDK_INT >= 28
                val name = if (hint == null || tooltip) label else "$label, $hint"
                if (tooltip && hint != null) info.tooltipText = hint
                if (host is EditText) {
                    if (Build.VERSION.SDK_INT >= 26) {
                        info.hintText = name
                    } else {
                        val typed = host.text
                        info.text = if (typed.isNullOrEmpty()) name else "$typed, $name"
                    }
                } else {
                    info.contentDescription = name
                }
            }
        }
    }

    fun enableScroll() {
        val scrollLayer = GoyaScrollLayer(context, scene, index)
        layer = scrollLayer
        addView(scrollLayer)
    }

    fun addCell(cell: GoyaCell) {
        val scrollLayer = layer
        if (scrollLayer != null) scrollLayer.addView(cell) else addView(cell)
    }

    fun fill(color: Int) {
        setBackgroundColor(color)
        val view = content
        if (view is Button || view is EditText) view.setBackgroundColor(color)
    }

    private var strokeColor = 0
    private var strokeWidth = -1
    private var border: GradientDrawable? = null

    /** A border drawn inside the cell, above its content. */
    fun stroke(color: Int) {
        strokeColor = color
        val drawable = GradientDrawable()
        drawable.setColor(0)
        border = drawable
        foreground = drawable
    }

    /** Text size in pixels (em size) and stroke width in pixels; -1 means none. */
    fun applyLengths(textPx: Float, strokePx: Int) {
        val view = content
        if (textPx >= 0f && view is TextView && view.textSize != textPx) {
            view.setTextSize(TypedValue.COMPLEX_UNIT_PX, textPx)
        }
        val drawable = border
        if (drawable != null && strokePx >= 0 && strokePx != strokeWidth) {
            strokeWidth = strokePx
            drawable.setStroke(strokePx, strokeColor)
        }
    }

    override fun onMeasure(widthSpec: Int, heightSpec: Int) {
        val width = MeasureSpec.getSize(widthSpec)
        val height = MeasureSpec.getSize(heightSpec)
        setMeasuredDimension(width, height)
        val exactWidth = MeasureSpec.makeMeasureSpec(width, MeasureSpec.EXACTLY)
        val exactHeight = MeasureSpec.makeMeasureSpec(height, MeasureSpec.EXACTLY)
        content?.measure(exactWidth, exactHeight)
        layer?.measure(exactWidth, exactHeight)
        for (k in 0 until childCount) {
            val child = getChildAt(k)
            if (child is GoyaCell) scene.measureCell(child)
        }
    }

    override fun onLayout(changed: Boolean, l: Int, t: Int, r: Int, b: Int) {
        content?.layout(0, 0, r - l, b - t)
        layer?.layout(0, 0, r - l, b - t)
        for (k in 0 until childCount) {
            val child = getChildAt(k)
            if (child is GoyaCell) scene.layoutCell(child, scene.left(index), scene.top(index))
        }
    }
}

/** Children of a scroll cell. Offsets are clamped to Frame::scroll_limits. */
class GoyaScrollLayer(context: Context, private val scene: GoyaSceneView, private val index: Int) : ViewGroup(context) {
    private val slop = ViewConfiguration.get(context).scaledTouchSlop
    private var lastX = 0f
    private var lastY = 0f
    private var dragging = false

    override fun onMeasure(widthSpec: Int, heightSpec: Int) {
        setMeasuredDimension(MeasureSpec.getSize(widthSpec), MeasureSpec.getSize(heightSpec))
        for (k in 0 until childCount) {
            val child = getChildAt(k)
            if (child is GoyaCell) scene.measureCell(child)
        }
    }

    override fun onLayout(changed: Boolean, l: Int, t: Int, r: Int, b: Int) {
        for (k in 0 until childCount) {
            val child = getChildAt(k)
            if (child is GoyaCell) scene.layoutCell(child, scene.left(index), scene.top(index))
        }
        scrollClamped(scrollX, scrollY)
    }

    fun scrollClamped(x: Int, y: Int) {
        scrollTo(x.coerceIn(0, scene.maxScrollX(index)), y.coerceIn(0, scene.maxScrollY(index)))
    }

    override fun canScrollVertically(direction: Int): Boolean =
        if (direction < 0) scrollY > 0 else scrollY < scene.maxScrollY(index)

    override fun canScrollHorizontally(direction: Int): Boolean =
        if (direction < 0) scrollX > 0 else scrollX < scene.maxScrollX(index)

    override fun getAccessibilityClassName(): CharSequence = "android.widget.ScrollView"

    override fun onInitializeAccessibilityNodeInfo(info: AccessibilityNodeInfo) {
        super.onInitializeAccessibilityNodeInfo(info)
        val forward = scrollY < scene.maxScrollY(index) || scrollX < scene.maxScrollX(index)
        val backward = scrollY > 0 || scrollX > 0
        info.isScrollable = forward || backward
        if (forward) info.addAction(AccessibilityNodeInfo.AccessibilityAction.ACTION_SCROLL_FORWARD)
        if (backward) info.addAction(AccessibilityNodeInfo.AccessibilityAction.ACTION_SCROLL_BACKWARD)
    }

    /** One page per action: vertical first, then horizontal. */
    override fun performAccessibilityAction(action: Int, arguments: Bundle?): Boolean {
        val pageX = maxOf(width, 1)
        val pageY = maxOf(height, 1)
        when (action) {
            AccessibilityNodeInfo.ACTION_SCROLL_FORWARD -> {
                if (scrollY < scene.maxScrollY(index)) scrollClamped(scrollX, scrollY + pageY)
                else if (scrollX < scene.maxScrollX(index)) scrollClamped(scrollX + pageX, scrollY)
                else return false
                return true
            }
            AccessibilityNodeInfo.ACTION_SCROLL_BACKWARD -> {
                if (scrollY > 0) scrollClamped(scrollX, scrollY - pageY)
                else if (scrollX > 0) scrollClamped(scrollX - pageX, scrollY)
                else return false
                return true
            }
        }
        return super.performAccessibilityAction(action, arguments)
    }

    override fun onInterceptTouchEvent(event: MotionEvent): Boolean {
        when (event.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                lastX = event.x
                lastY = event.y
                dragging = false
            }
            MotionEvent.ACTION_MOVE -> {
                if (!dragging && (Math.abs(event.x - lastX) > slop || Math.abs(event.y - lastY) > slop)) {
                    dragging = true
                    lastX = event.x
                    lastY = event.y
                }
            }
        }
        return dragging
    }

    override fun onTouchEvent(event: MotionEvent): Boolean {
        when (event.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                lastX = event.x
                lastY = event.y
            }
            MotionEvent.ACTION_MOVE -> {
                scrollClamped(scrollX + (lastX - event.x).toInt(), scrollY + (lastY - event.y).toInt())
                lastX = event.x
                lastY = event.y
            }
            MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> dragging = false
        }
        return true
    }
}
"#;
