package io.paritytech.polkadotapp.bandersnatch_crypto;

public class CreateProofResult {

    public final byte[] proof;
    public final byte[] alias;

    public CreateProofResult(byte[] proof, byte[] alias) {
        this.proof = proof;
        this.alias = alias;
    }
}
