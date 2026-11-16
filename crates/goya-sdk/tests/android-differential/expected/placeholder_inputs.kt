// Expected Kotlin output for placeholder_inputs.gscene test case
import android.content.Context
import android.widget.TextView
import android.text.Editable
import android.text.TextWatcher

class GoyaRootInputPreview(context: Context) : TextView(context) {
    init {
        // Name input with placeholder
        val nameView = TextView(context)
        nameView.text = "John Doe"
        nameView.setHint("Enter full name")
        
        // Email input with placeholder
        val emailView = TextView(context)
        emailView.text = "john@example.com"
        emailView.setHint("name@example.com")
        emailView.setInputType(InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_EMAIL_ADDRESS)
        
        // Phone number input with placeholder
        val phoneView = TextView(context)
        phoneView.text = "+1234567890"
        phoneView.setHint("(123) 456-7890")
        
        // Address input with placeholder
        val addressView = TextView(context)
        addressView.text = "123 Main St, City, State 12345"
        addressView.setHint("123 Main St, Anytown, USA")
        
        // Comments input with placeholder
        val commentsView = TextView(context)
        commentsView.text = "This is a comment"
        commentsView.setHint("Add a comment...")
        
        // Add real-time validation listeners
        nameView.addTextChangedListener(object : TextWatcher {
            override fun beforeTextChanged(s: CharSequence?, start: Int, count: Int, after: Int) {}
            override fun onTextChanged(s: CharSequence?, start: Int, before: Int, count: Int) {}
            override fun afterTextChanged(s: Editable?) {
                validateInput(s?.toString() ?: "")
            }
        })
    }
}

fun validateInput(text: String): Boolean {
    return !text.isNullOrEmpty() && text.length >= 3
}

fun showValidationError(errorMessage: String) {
    // Show error message to user
    // Typically displayed in errorTextView
}