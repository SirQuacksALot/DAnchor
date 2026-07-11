plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.plugin.compose")
}

android {
    namespace = "com.danchor.app"
    compileSdk = 36

    defaultConfig {
        applicationId = "com.danchor.app"
        minSdk = 26
        targetSdk = 36
        versionCode = 1
        versionName = "0.1.0"
    }

    buildTypes {
        release {
            isMinifyEnabled = false
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    buildFeatures {
        compose = true
    }

    sourceSets["main"].kotlin.srcDir("../generated")
}

dependencies {
    // Pinned below the version that requires compileSdk 37 (Android 17
    // beta, which enforces the not-yet-stable local-network-permission
    // gate) - see .ai/tasks.toon for the full story.
    implementation(platform("androidx.compose:compose-bom:2026.03.01"))
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.ui:ui-graphics")
    implementation("androidx.compose.ui:ui-tooling-preview")
    implementation("androidx.compose.material3:material3")
    // Extended (not just -core) so device/connection-type icons like
    // Cable/CableOff are available - core only bundles ~48 basic icons.
    implementation("androidx.compose.material:material-icons-extended")
    implementation("androidx.activity:activity-compose:1.13.0")
    implementation("androidx.core:core-ktx:1.17.0")
    // Per-app language switching (AppCompatDelegate.setApplicationLocales) -
    // works without MainActivity extending AppCompatActivity via the
    // AppLocalesMetadataHolderService auto-store hook in AndroidManifest.xml.
    implementation("androidx.appcompat:appcompat:1.7.0")
    // 2.10.0, not the latest 2.11.0: that one pulls in lifecycle-runtime-
    // compose-android which requires compileSdk 37 (see the compose-bom
    // comment above for why we're avoiding that).
    implementation("androidx.lifecycle:lifecycle-runtime-ktx:2.10.0")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.11.0")
    // Runtime the UniFFI-generated Kotlin bindings need to call into libdanchor_ffi.so.
    implementation("net.java.dev.jna:jna:5.19.1@aar")
}
