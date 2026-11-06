plugins {
    id("com.android.library") version "8.7.3"
    id("org.jetbrains.kotlin.android") version "2.0.21"
}

android {
    namespace = "dev.goya.platform.ime"
    compileSdk = 35

    defaultConfig {
        minSdk = 23
    }

    sourceSets {
        getByName("main") {
            manifest.srcFile("src/main/AndroidManifest.xml")
            java.srcDirs("../ime/src/main/kotlin", "../ime/src/androidMain/kotlin")
        }
        getByName("test") {
            java.srcDirs("../ime/src/test/kotlin")
        }
    }

    testOptions {
        unitTests.isReturnDefaultValues = true
    }
}

dependencies {
    testImplementation("junit:junit:4.13.2")
}

// The existing JVM checks are a plain Kotlin main, so run them without an
// emulator and include them in the normal `check` lifecycle.
tasks.register<JavaExec>("imeJvmTest") {
    group = "verification"
    description = "Runs the IME adapter's dependency-free JVM checks."
    dependsOn("testClasses")
    classpath = sourceSets["test"].runtimeClasspath
    mainClass.set("dev.goya.platform.ime.GoyaImeAdapterTest")
}

tasks.named("check") {
    dependsOn("imeJvmTest")
}
