import java.util.Properties
import java.io.FileInputStream

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

// Load .env file
val envFile = rootProject.file(".env")
val envProperties = Properties()
if (envFile.exists()) {
    FileInputStream(envFile).use { envProperties.load(it) }
}

fun getEnvProperty(key: String, default: String = ""): String {
    return envProperties.getProperty(key) ?: System.getenv(key) ?: default
}

android {
    namespace = "eu.trahe.geodb"
    compileSdk = 35

    // Signing configuration from .env
    signingConfigs {
        create("release") {
            val keystoreFile = getEnvProperty("KEYSTORE_FILE")
            if (keystoreFile.isNotEmpty()) {
                storeFile = file(keystoreFile)
                storePassword = getEnvProperty("KEYSTORE_PASSWORD")
                keyAlias = getEnvProperty("KEY_ALIAS", "geodb")
                keyPassword = getEnvProperty("KEY_PASSWORD")
            }
        }
    }

    defaultConfig {
        applicationId = "eu.trahe.geodb"
        minSdk = 24
        targetSdk = 35
        versionCode = 6
        versionName = "0.1.5"

        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"

        ndk {
            abiFilters += listOf("arm64-v8a", "armeabi-v7a", "x86_64", "x86")
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro"
            )
            // Use signing config if keystore is configured
            if (envFile.exists() && getEnvProperty("KEYSTORE_FILE").isNotEmpty()) {
                signingConfig = signingConfigs.getByName("release")
            }
        }
        debug {
            isMinifyEnabled = false
        }
    }

    // Note: ABI splits are disabled because they conflict with AAB (Android App Bundle) builds
    // The AAB already handles per-ABI optimization when uploading to Play Store
    // If you need split APKs for direct distribution, build with assembleRelease instead of bundleRelease
    splits {
        abi {
            isEnable = false  // Disabled for AAB compatibility
            reset()
            include("arm64-v8a", "armeabi-v7a", "x86_64", "x86")
            isUniversalApk = true
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_1_8
        targetCompatibility = JavaVersion.VERSION_1_8
    }

    kotlinOptions {
        jvmTarget = "1.8"
    }

    buildFeatures {
        viewBinding = true
        compose = true
    }

    composeOptions {
        kotlinCompilerExtensionVersion = "1.5.6"
    }

    sourceSets {
        getByName("main") {
            jniLibs.srcDirs("src/main/jniLibs")
        }
    }
}

dependencies {
    // Core Android
    implementation("androidx.core:core-ktx:1.12.0")
    implementation("androidx.lifecycle:lifecycle-runtime-ktx:2.6.2")
    implementation("androidx.activity:activity-compose:1.8.2")

    // Compose
    implementation(platform("androidx.compose:compose-bom:2023.10.01"))
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.ui:ui-graphics")
    implementation("androidx.compose.ui:ui-tooling-preview")
    implementation("androidx.compose.material3:material3")

    // JNA for UniFFI
    implementation("net.java.dev.jna:jna:5.14.0@aar")

    // Testing
    testImplementation("junit:junit:4.13.2")
    androidTestImplementation("androidx.test.ext:junit:1.1.5")
    androidTestImplementation("androidx.test.espresso:espresso-core:3.5.1")
    androidTestImplementation(platform("androidx.compose:compose-bom:2023.10.01"))
    androidTestImplementation("androidx.compose.ui:ui-test-junit4")
    debugImplementation("androidx.compose.ui:ui-tooling")
    debugImplementation("androidx.compose.ui:ui-test-manifest")
}
