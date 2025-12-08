# Add project specific ProGuard rules here.

# Keep JNA classes (required for UniFFI)
-keep class com.sun.jna.** { *; }
-keep class * implements com.sun.jna.** { *; }
-dontwarn com.sun.jna.**

# Keep UniFFI generated classes
-keep class uniffi.** { *; }
-keep interface uniffi.** { *; }

# Keep native methods
-keepclasseswithmembernames class * {
    native <methods>;
}

# Keep Kotlin metadata for reflection
-keep class kotlin.Metadata { *; }

# Keep Compose classes (if using Compose runtime features)
-keep class androidx.compose.** { *; }
-dontwarn androidx.compose.**

# Keep app-specific classes from obfuscation
-keep class eu.trahe.geodb.** { *; }

# Print mapping file for crash reporting
-printmapping mapping.txt
