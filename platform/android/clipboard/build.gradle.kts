plugins {
    id("com.android.library") version "8.7.3"
    id("org.jetbrains.kotlin.android") version "2.0.21"
}
android {
    namespace = "dev.goya.platform.clipboard"
    compileSdk = 35
    defaultConfig { minSdk = 23 }
}
dependencies { testImplementation("junit:junit:4.13.2") }
