// Run scripts/build-android.sh first: it puts the Rust library in src/main/jniLibs and the
// generated Kotlin bindings in src/main/java/dev/lysen/not/core.
plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.plugin.compose")
}

android {
    namespace = "dev.lysen.not"
    compileSdk = 37

    defaultConfig {
        applicationId = "dev.lysen.not"
        minSdk = 29
        targetSdk = 36
        versionCode = 1
        versionName = "0.1.0"
    }

    buildFeatures {
        compose = true
        buildConfig = true
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
}

dependencies {
    implementation(platform("androidx.compose:compose-bom:2026.09.00"))
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.material3:material3")
    implementation("androidx.activity:activity-compose:1.13.0")
    implementation("androidx.lifecycle:lifecycle-viewmodel-compose:2.11.0")
    implementation("androidx.lifecycle:lifecycle-runtime-compose:2.11.0")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.11.0")
    // uniffi's Kotlin bindings call into the Rust library through JNA.
    implementation("net.java.dev.jna:jna:5.19.1@aar")
    debugImplementation("androidx.compose.ui:ui-tooling")
}
