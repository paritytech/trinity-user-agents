package io.paritytech.polkadotapp.tools_ipfs_api;

import io.ipfs.multibase.*;
import io.ipfs.multihash.*;

import java.io.*;
import java.util.*;

// Adapted from https://github.com/ipld/java-cid/blob/master/src/main/java/io/ipfs/cid/Cid.java
// Changes:
// - Added Json codec
// - Added cast(InputStream) overload for streaming CID parsing (e.g., CAR blocks where CID
//   is followed by data). Consumes only the CID bytes, leaving the rest in the stream.
//   Correctly handles both CIDv0 (raw sha2-256 multihash) and CIDv1 (version + codec + multihash).
public class Cid extends Multihash {

    public static final class CidEncodingException extends RuntimeException {

        public CidEncodingException(String message) {
            super(message);
        }

        public CidEncodingException(String message, Throwable cause) {
            super(message, cause);
        }
    }

    public enum Codec {
        Cbor(0x51),
        Raw(0x55),
        DagProtobuf(0x70),
        DagCbor(0x71),
        Libp2pKey(0x72),
        EthereumBlock(0x90),
        EthereumTx(0x91),
        BitcoinBlock(0xb0),
        BitcoinTx(0xb1),
        ZcashBlock(0xc0),
        ZcashTx(0xc1),

        Json(0x0200);

        public long type;

        Codec(long type) {
            this.type = type;
        }

        private static Map<Long, Codec> lookup = new TreeMap<>();
        private static Map<String, Codec> nameLookup = new TreeMap<>();
        static {
            for (Codec c: Codec.values())
                lookup.put(c.type, c);
            // https://github.com/multiformats/multicodec/blob/master/table.csv
            nameLookup.put("cbor", Cbor);
            nameLookup.put("raw", Raw);
            nameLookup.put("dag-pb", DagProtobuf);
            nameLookup.put("dag-cbor", DagCbor);
            nameLookup.put("libp2p-key", Libp2pKey);
            nameLookup.put("eth-block", EthereumBlock);
            nameLookup.put("eth-block-list", EthereumTx);
            nameLookup.put("bitcoin-block", BitcoinBlock);
            nameLookup.put("bitcoin-tx", BitcoinTx);
            nameLookup.put("zcash-block", ZcashBlock);
            nameLookup.put("zcash-tx", ZcashTx);
            nameLookup.put("json", Json);
        }

        public static Codec lookup(long c) {
            if (!lookup.containsKey(c))
                throw new IllegalStateException("Unknown Codec type: " + c);
            return lookup.get(c);
        }
        public static Codec lookupIPLDName(String name) {
            if (!nameLookup.containsKey(name))
                throw new IllegalStateException("Unknown Codec type: " + name);
            return nameLookup.get(name);
        }
    }

    public final long version;
    public final Codec codec;

    public Cid(long version, Codec codec, Multihash.Type type, byte[] hash) {
        super(type, hash);
        this.version = version;
        this.codec = codec;
    }

    public static Cid build(long version, Codec codec, Multihash h) {
        return new Cid(version, codec, h.getType(), h.getHash());
    }

    private byte[] toBytesV0() {
        return super.toBytes();
    }

    private byte[] toBytesV1() {
        try {
            ByteArrayOutputStream res = new ByteArrayOutputStream();
            putUvarint(res, version);
            putUvarint(res, codec.type);
            super.serialize(res);
            return res.toByteArray();
        } catch (IOException e) {
            throw new RuntimeException(e);
        }
    }

    @Override
    public byte[] toBytes() {
        if (version == 0)
            return toBytesV0();
        else if (version == 1)
            return toBytesV1();
        throw new IllegalStateException("Unknown cid version: " + version);
    }

    @Override
    public String toString() {
        if (version == 0) {
            return super.toString();
        } else if (version == 1) {
            return Multibase.encode(Multibase.Base.Base32, toBytesV1());
        }
        throw new IllegalStateException("Unknown Cid version: " + version);
    }

    @Override
    public Multihash bareMultihash() {
        return new Multihash(getType(), getHash());
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) return true;
        if (! (o instanceof Multihash)) return false;
        if (!super.equals(o)) return false;

        if (o instanceof Cid) {
            Cid cid = (Cid) o;

            if (version != cid.version) return false;
            return codec == cid.codec;
        }
        // o must be a Multihash
        return version == 0 && super.equals(o);
    }

    @Override
    public int hashCode() {
        int result = super.hashCode();
        if (version == 0)
            return result;
        result = 31 * result + (int) (version ^ (version >>> 32));
        result = 31 * result + (codec != null ? codec.hashCode() : 0);
        return result;
    }

    public static Cid buildV0(Multihash h) {
        return Cid.build(0, Codec.DagProtobuf, h);
    }

    public static Cid buildCidV1(Codec c, Multihash.Type type, byte[] hash) {
        return new Cid(1, c, type, hash);
    }

    public static Cid decode(String v) {
        if (v.length() < 2)
            throw new IllegalStateException("Cid too short!");

        // support legacy format
        if (v.length() == 46 && v.startsWith("Qm"))
            return buildV0(Multihash.fromBase58(v));

        byte[] data = Multibase.decode(v);
        return cast(data);
    }

    public static Cid cast(byte[] data) {
        return cast(new ByteArrayInputStream(data));
    }

    /**
     * Reads a CID from the stream, consuming only the bytes that belong to the CID.
     * This is useful when the stream contains a CID followed by other data (e.g., CAR blocks).
     *
     * CIDv0 is always dag-pb + sha2-256 (hash code 0x12), so the first byte is always 0x12.
     * CIDv1 starts with version varint 1, followed by a codec varint — the second varint
     * is always > 1 (smallest codec is 0x51), so (1, >1) is unambiguously CIDv1.
     *
     * We detect CIDv0 by checking if the first varint equals 0x12 (sha2-256) and the second
     * varint equals 0x20 (32 bytes), which is the only valid CIDv0 multihash prefix.
     */
    public static Cid cast(InputStream in) {
        try {
            if (!in.markSupported())
                in = new BufferedInputStream(in, 2);

            in.mark(2);
            int first = in.read();
            int second = in.read();
            in.reset();

            if (first == 0x12 && second == 0x20) {
                // CIDv0: raw sha2-256 multihash (0x12 = sha2-256, 0x20 = 32 byte digest)
                Multihash hash = Multihash.deserialize(in);
                return new Cid(0, Codec.DagProtobuf, hash.getType(), hash.getHash());
            }

            // CIDv1: version varint + codec varint + multihash
            long version = readVarint(in);
            if (version != 1)
                throw new CidEncodingException("Invalid Cid version number: " + version);

            long codec = readVarint(in);
            Multihash hash = Multihash.deserialize(in);
            return new Cid(1, Codec.lookup(codec), hash.getType(), hash.getHash());
        } catch (CidEncodingException cee) {
            throw cee;
        } catch (Exception e) {
            throw new CidEncodingException("Invalid cid bytes from stream", e);
        }
    }

    private static String[] HEX_DIGITS = new String[]{
            "0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "a", "b", "c", "d", "e", "f"};
    private static String[] HEX = new String[256];
    static {
        for (int i=0; i < 256; i++)
            HEX[i] = HEX_DIGITS[(i >> 4) & 0xF] + HEX_DIGITS[i & 0xF];
    }

    private static String byteToHex(byte b) {
        return HEX[b & 0xFF];
    }

    private static String bytesToHex(byte[] data) {
        StringBuilder s = new StringBuilder();
        for (byte b : data)
            s.append(byteToHex(b));
        return s.toString();
    }
}
