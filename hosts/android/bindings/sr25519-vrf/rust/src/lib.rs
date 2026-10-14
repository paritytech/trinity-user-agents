#![allow(non_snake_case)]

extern crate jni;
extern crate merlin;
extern crate schnorrkel;

use jni::errors::Result as JniResult;
use jni::objects::JClass;
use jni::sys::{jbyteArray, jobjectArray};
use jni::JNIEnv;
use merlin::Transcript;
use schnorrkel::Keypair;
use std::mem;
use std::ptr;

macro_rules! try_or_throw {
    ($jni_env:ident, $expr:expr, $ret:expr) => {
        match $expr {
            JniResult::Ok(val) => val,
            JniResult::Err(err) => {
                $jni_env
                    .throw_new("java/lang/Exception", err.description())
                    .unwrap();
                return $ret;
            }
        }
    };
}

macro_rules! try_or_throw_null {
    ($jni_env:ident, $expr:expr) => {
        try_or_throw!($jni_env, $expr, ptr::null_mut())
    };
}

macro_rules! throw_null {
    ($jni_env:ident, $message:expr) => {{
        $jni_env.throw_new("java/lang/Exception", $message).unwrap();
        return ptr::null_mut();
    }};
}

/// Merlin types its labels `&'static [u8]` as misuse-resistance — it nudges a protocol author to
/// hard-code their labels — while the binding guarantee comes from STROBE-128's tag-length-value
/// framing, which encodes runtime-length labels just as unambiguously (RFC-0023 § Implementation
/// notes). Sound because merlin absorbs the label synchronously and never retains the reference.
unsafe fn as_static_label(label: &[u8]) -> &'static [u8] {
    mem::transmute::<&[u8], &'static [u8]>(label)
}

/// Replays a transcript recipe: `Transcript::new(label)` followed by one `append_message` per item,
/// in order. Mirrors `sp_core::sr25519::vrf::VrfTranscript::new(label, &[(key, value), ..])`, so a
/// consuming runtime that builds the same recipe reconstructs the same transcript.
fn build_transcript(label: &[u8], items: &[(Vec<u8>, Vec<u8>)]) -> Transcript {
    let mut transcript = Transcript::new(unsafe { as_static_label(label) });

    for (item_label, value) in items {
        transcript.append_message(unsafe { as_static_label(item_label) }, value);
    }

    transcript
}

/// Reads a `byte[][]` into owned byte vectors.
fn read_byte_array_array(jni_env: &JNIEnv, array: jobjectArray) -> JniResult<Vec<Vec<u8>>> {
    let length = jni_env.get_array_length(array)?;
    let mut result = Vec::with_capacity(length as usize);

    for index in 0..length {
        let element = jni_env.get_object_array_element(array, index)?;
        result.push(jni_env.convert_byte_array(element.into_inner())?);
    }

    Ok(result)
}

/// Signs an sr25519 (schnorrkel) VRF over a caller-supplied merlin transcript with a 96-byte
/// keypair (`rawSecretKey(64) ++ rawPublicKey(32)`). `itemLabels` and `itemValues` are parallel
/// arrays forming the ordered `append_message` calls. Returns the 96-byte output
/// `preOutput(32) ++ proof(64)`.
#[no_mangle]
fn Java_io_paritytech_polkadotapp_sr25519_1vrf_Sr25519VrfCrypto_sign(
    jni_env: JNIEnv,
    _: JClass,
    keypair: jbyteArray,
    transcript_label: jbyteArray,
    item_labels: jobjectArray,
    item_values: jobjectArray,
) -> jbyteArray {
    let keypair_bytes = try_or_throw_null!(jni_env, jni_env.convert_byte_array(keypair));
    let label_bytes = try_or_throw_null!(jni_env, jni_env.convert_byte_array(transcript_label));
    let labels = try_or_throw_null!(jni_env, read_byte_array_array(&jni_env, item_labels));
    let values = try_or_throw_null!(jni_env, read_byte_array_array(&jni_env, item_values));

    if labels.len() != values.len() {
        throw_null!(
            jni_env,
            "transcript item labels and values must have equal length"
        );
    }

    let keypair = match Keypair::from_bytes(&keypair_bytes) {
        Ok(keypair) => keypair,
        Err(_) => throw_null!(jni_env, "invalid sr25519 keypair"),
    };

    let items: Vec<(Vec<u8>, Vec<u8>)> = labels.into_iter().zip(values).collect();
    let (in_out, proof, _) = keypair.vrf_sign(build_transcript(&label_bytes, &items));

    let mut out = Vec::with_capacity(96);
    out.extend_from_slice(&in_out.to_preout().to_bytes());
    out.extend_from_slice(&proof.to_bytes());

    try_or_throw_null!(jni_env, jni_env.byte_array_from_slice(&out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use schnorrkel::{ExpansionMode, Keypair, MiniSecretKey};

    /// Merlin domain-separation label of the People Chain airdrop, matching the runtime's
    /// `indiv_pallet_airdrop::vrf::VRF_TRANSCRIPT_LABEL`, kept here as the regression fixture.
    const AIRDROP_TRANSCRIPT_LABEL: &[u8] = b"pop:airdrop";

    /// `event_id(game_index)` = `"pop:game:airdrop:" ++ 11 spaces ++ game_index.to_be_bytes()`
    /// = 28 + 4 = 32 bytes (matches the runtime's event id).
    fn event_id(game_index: u32) -> Vec<u8> {
        let mut id = b"pop:game:airdrop:           ".to_vec();
        id.extend_from_slice(&game_index.to_be_bytes());
        assert_eq!(id.len(), 32);
        id
    }

    /// The airdrop recipe as the Kotlin caller now supplies it.
    fn airdrop_items(event_id: &[u8], public_key: &[u8]) -> Vec<(Vec<u8>, Vec<u8>)> {
        let domain = [AIRDROP_TRANSCRIPT_LABEL, event_id].concat();

        vec![
            (b"domain".to_vec(), domain),
            (b"signer".to_vec(), public_key.to_vec()),
        ]
    }

    /// The construction this crate hard-coded before RFC-0023 generalized it — kept verbatim as the
    /// oracle for `airdrop_shape_matches_previous_hardcoded`.
    fn transcript_for_event_hardcoded(event_id: &[u8], public_key: &[u8]) -> Transcript {
        let mut domain = Vec::with_capacity(AIRDROP_TRANSCRIPT_LABEL.len() + event_id.len());
        domain.extend_from_slice(AIRDROP_TRANSCRIPT_LABEL);
        domain.extend_from_slice(event_id);

        let mut transcript = Transcript::new(b"pop:airdrop");
        transcript.append_message(b"domain", &domain);
        transcript.append_message(b"signer", public_key);
        transcript
    }

    fn challenge_of(mut transcript: Transcript) -> [u8; 32] {
        let mut challenge = [0u8; 32];
        transcript.challenge_bytes(b"eq-check", &mut challenge);
        challenge
    }

    #[test]
    fn signs_and_self_verifies() {
        let keypair = Keypair::generate();
        let items = airdrop_items(&event_id(7), &keypair.public.to_bytes());

        let (in_out, proof, _) =
            keypair.vrf_sign(build_transcript(AIRDROP_TRANSCRIPT_LABEL, &items));

        let verify = keypair.public.vrf_verify(
            build_transcript(AIRDROP_TRANSCRIPT_LABEL, &items),
            &in_out.to_preout(),
            &proof,
        );
        assert!(verify.is_ok());
    }

    /// Proves the generic replayer is byte-identical to sp-core's `VrfTranscript` — the construction
    /// consuming runtimes use. If the two transcripts agree, the challenge bytes drawn from each are
    /// equal; any difference in label, order, or framing diverges here. This is also what shows the
    /// `'static` label cast produces stock-merlin bytes.
    #[test]
    fn generic_transcript_matches_sp_core() {
        use sp_core::sr25519::vrf::VrfTranscript;

        let label = b"test:generic-vrf";
        let items: Vec<(Vec<u8>, Vec<u8>)> = vec![
            (b"empty".to_vec(), Vec::new()),
            (b"short".to_vec(), b"a".to_vec()),
            (b"long".to_vec(), vec![0xABu8; 512]),
        ];

        let sp_transcript: Transcript = VrfTranscript::new(
            label,
            &[
                (b"empty", &[][..]),
                (b"short", &b"a"[..]),
                (b"long", &[0xABu8; 512][..]),
            ],
        )
        .0;

        assert_eq!(
            challenge_of(build_transcript(label, &items)),
            challenge_of(sp_transcript)
        );
    }

    /// Regression guard for the RFC-0023 rename: the airdrop recipe, now assembled by the Kotlin
    /// caller and replayed generically, must reproduce the transcript this crate used to hard-code.
    /// A slip here verifies fine locally but silently invalidates every lottery ticket on chain.
    #[test]
    fn airdrop_shape_matches_previous_hardcoded() {
        let public_key = Keypair::generate().public.to_bytes();
        let eid = event_id(42);

        assert_eq!(
            challenge_of(build_transcript(
                AIRDROP_TRANSCRIPT_LABEL,
                &airdrop_items(&eid, &public_key)
            )),
            challenge_of(transcript_for_event_hardcoded(&eid, &public_key))
        );
    }

    /// Full end-to-end parity with iOS (`sr25519_generic_vrf_sign`): the 96-byte output we emit must
    /// (a) have a pre-output byte-equal to sp-core's `Pair::vrf_sign`, and (b) decode as sp-core's
    /// `VrfSignature` and pass `vrf_verify` — i.e. the runtime would accept it. Catches signature or
    /// encoding drift a future schnorrkel bump could introduce, which the transcript-only test would
    /// miss.
    #[test]
    fn generic_vrf_verifies_under_sp_core() {
        use parity_scale_codec::Decode;
        use sp_core::crypto::{VrfPublic, VrfSecret};
        use sp_core::sr25519::vrf::{VrfSignature, VrfTranscript};
        use sp_core::sr25519::Pair;
        use sp_core::Pair as _;

        let seed: [u8; 32] = *b"12345678901234567890123456789012";
        let eid = event_id(7);

        // Our signing path — identical to the JNI fn.
        let keypair = MiniSecretKey::from_bytes(&seed)
            .expect("mini secret key")
            .expand_to_keypair(ExpansionMode::Ed25519);
        let public = keypair.public.to_bytes();
        let items = airdrop_items(&eid, &public);
        let (in_out, proof, _) =
            keypair.vrf_sign(build_transcript(AIRDROP_TRANSCRIPT_LABEL, &items));
        let mut out = Vec::with_capacity(96);
        out.extend_from_slice(&in_out.to_preout().to_bytes());
        out.extend_from_slice(&proof.to_bytes());

        // sp-core oracle.
        let pair = Pair::from_seed(&seed);
        let sp_public = pair.public();
        assert_eq!(public, sp_public.0, "public keys must match");

        let domain = [AIRDROP_TRANSCRIPT_LABEL, eid.as_slice()].concat();
        let sign_data = VrfTranscript::new(
            b"pop:airdrop",
            &[(b"domain", domain.as_slice()), (b"signer", &sp_public.0)],
        )
        .into_sign_data();

        let reference = pair.vrf_sign(&sign_data);
        assert_eq!(
            &out[..32],
            &reference.pre_output.0.to_bytes()[..],
            "pre-output must match sp-core"
        );

        let signature = VrfSignature::decode(&mut &out[..]).expect("decode VrfSignature");
        assert!(
            sp_public.vrf_verify(&sign_data, &signature),
            "runtime must verify our signature"
        );
    }
}
