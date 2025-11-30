# Add project specific ProGuard rules here.

# Keep JNA classes
-keep class com.sun.jna.** { *; }
-keep class * implements com.sun.jna.** { *; }

# Keep UniFFI generated classes
-keep class uniffi.** { *; }
