-dontwarn java.beans.ConstructorProperties
-dontwarn java.beans.Transient
-dontwarn lombok.Generated

-keep class java.security.** { *; }
-keep class org.bouncycastle.** { *; }
-keep class net.jpountz.xxhash.** { *; }

-keep class io.paritytech.polkadotapp.chains.** { *; }
-keep class io.paritytech.polkadotapp.feature_xcm_impl.** { *; }
-keep class io.paritytech.polkadotapp.feature_statement_store_impl.data.models.response.** { *; }

-keep class io.novasama.substrate_sdk_android.** { *; }

-keepclassmembers class * implements androidx.viewbinding.ViewBinding {
    public static *** bind(android.view.View);
    public static *** inflate(android.view.LayoutInflater);
    public static *** inflate(android.view.LayoutInflater, android.view.ViewGroup, boolean);
}