package io.paritytech.polkadotapp.bandersnatch_crypto;

import java.util.List;

public class BandersnatchCrypto {

    static {
        System.loadLibrary("bandersnatch_crypto_java");
    }

    public static native byte[] derive_member_key(byte[] entropy);

    public static native CreateProofResult create_proof(
        byte[] entropy,
        List<byte[]> members,
        byte[] context,
        byte[] message,
        int domainSize
    );

    public static native byte[] sign(byte[] entropy, byte[] message);

    public static native byte[] alias_in_context(byte[] entropy, byte[] context);
}
