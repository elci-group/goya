// Expected Kotlin output for max_length_inputs.gscene test case
import android.content.Context
import android.widget.TextView
import android.text.InputFilter

class GoyaRootInputPreview(context: Context) : TextView(context) {
    init {
        // Username with max length 50
        val usernameView = TextView(context)
        usernameView.text = "a".repeat(50)
        usernameView.setFilters(arrayOf(InputFilter.LengthFilter(50)))
        usernameView.setHint("Username")
        
        // Password with max length 20
        val passwordView = TextView(context)
        passwordView.text = "b".repeat(20)
        passwordView.setFilters(arrayOf(InputFilter.LengthFilter(20)))
        passwordView.setHint("Password")
        passwordView.setInputType(InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_PASSWORD)
        
        // Email with max length 100
        val emailView = TextView(context)
        emailView.text = "c".repeat(100)
        emailView.setFilters(arrayOf(InputFilter.LengthFilter(100)))
        emailView.setHint("Email")
        emailView.setInputType(InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_EMAIL_ADDRESS)
        
        // Very long text field with max length 200
        val longTextView = TextView(context)
        longTextView.text = "x".repeat(200)
        longTextView.setFilters(arrayOf(InputFilter.LengthFilter(200)))
        longTextView.setHint("Long text")
        
        // Short text field with max length 5
        val shortView = TextView(context)
        shortView.text = "xyz"
        shortView.setFilters(arrayOf(InputFilter.LengthFilter(5)))
        shortView.setHint("Short")
        
        // Validation function for max-length checking
        fun validateMaxLength(text: String, maxLength: Int): Boolean {
            return text.length <= maxLength
        }
        
        // Real-time length validation
        fun onTextChanged(text: String, maxLength: Int) {
            val isValid = validateMaxLength(text, maxLength)
            if (!isValid) {
                showError("Maximum length exceeded")
            }
        }
        
        fun showError(message: String) {
            // Show error message to user
            // Typically displayed in errorTextView with animation
        }
    }
}