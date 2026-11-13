package dev.goya.platform.clipboard

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.os.Looper

enum class ClipboardOperation { COPY, CUT, PASTE }
enum class ClipboardFailure { DENIED, WRONG_THREAD, UNAVAILABLE, UNSUPPORTED, INVALID_TEXT, TOO_LARGE }
sealed class ClipboardResult<out T> {
    data class Success<T>(val value: T) : ClipboardResult<T>()
    data class Failure(val reason: ClipboardFailure) : ClipboardResult<Nothing>()
    data object NoChange : ClipboardResult<Nothing>()
}

/** Explicit transport only. The host must authorize each user action and own editor commits. */
class GoyaClipboard(
    context: Context,
    private val authorize: (ClipboardOperation) -> Boolean = { false },
) {
    private val manager = context.applicationContext.getSystemService(Context.CLIPBOARD_SERVICE) as? ClipboardManager

    /** Success means Android accepted the write call, not that another app read it. */
    fun write(operation: ClipboardOperation, text: String): ClipboardResult<Unit> =
        writeText(operation, text, ::checkAccess) {
            val clipboard = manager ?: return failure(ClipboardFailure.UNAVAILABLE)
            clipboard.setPrimaryClip(ClipData.newPlainText("", text))
            ClipboardResult.Success(Unit)
        }

    /** Reads are disabled until bounded native acquisition is available. */
    fun read(): ClipboardResult<String> = readText(::checkAccess)

    private fun checkAccess(operation: ClipboardOperation): ClipboardResult.Failure? {
        if (Looper.myLooper() != Looper.getMainLooper()) return failure(ClipboardFailure.WRONG_THREAD)
        return try { if (authorize(operation)) null else failure(ClipboardFailure.DENIED) }
        catch (_: RuntimeException) { failure(ClipboardFailure.DENIED) }
    }
}

private fun failure(reason: ClipboardFailure) = ClipboardResult.Failure(reason)

internal inline fun writeText(
    operation: ClipboardOperation,
    text: String,
    access: (ClipboardOperation) -> ClipboardResult.Failure?,
    nativeWrite: () -> ClipboardResult<Unit>,
): ClipboardResult<Unit> {
    if (operation != ClipboardOperation.COPY && operation != ClipboardOperation.CUT) return failure(ClipboardFailure.DENIED)
    access(operation)?.let { return it }
    if (text.isEmpty()) return ClipboardResult.NoChange
    validateText(text)?.let { return failure(it) }
    return try { nativeWrite() } catch (_: RuntimeException) { failure(ClipboardFailure.UNAVAILABLE) }
}

internal fun readText(access: (ClipboardOperation) -> ClipboardResult.Failure?): ClipboardResult<String> =
    access(ClipboardOperation.PASTE) ?: failure(ClipboardFailure.UNSUPPORTED)

/** Counts UTF-8 bytes without a second payload allocation and rejects unpaired surrogates. */
internal fun validateText(text: String): ClipboardFailure? {
    val limit = 1024 * 1024
    var bytes = 0
    var index = 0
    while (index < text.length) {
        val c = text[index]
        val size = when {
            c.isHighSurrogate() -> {
                if (index + 1 >= text.length || !text[index + 1].isLowSurrogate()) return ClipboardFailure.INVALID_TEXT
                index++
                4
            }
            c.isLowSurrogate() -> return ClipboardFailure.INVALID_TEXT
            c.code < 0x80 -> 1
            c.code < 0x800 -> 2
            else -> 3
        }
        bytes += size
        if (bytes > limit) return ClipboardFailure.TOO_LARGE
        index++
    }
    return null
}
