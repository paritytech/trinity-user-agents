package io.paritytech.polkadotapp.sr25519_vrf;

public class Sr25519VrfCrypto {

    static {
        System.loadLibrary("sr25519_vrf_java");
    }

    // keypair: 96 bytes (rawSecretKey 64 ++ rawPublicKey 32).
    // itemLabels and itemValues are parallel arrays forming the ordered append_message calls.
    // Returns 96 bytes: preOutput 32 ++ proof 64.
    public static native byte[] sign(
            byte[] keypair,
            byte[] transcriptLabel,
            byte[][] itemLabels,
            byte[][] itemValues
    );
}
