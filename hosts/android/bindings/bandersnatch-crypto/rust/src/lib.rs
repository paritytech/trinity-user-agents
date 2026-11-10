#![allow(non_snake_case)]

extern crate core;
extern crate jni;
extern crate verifiable;

use jni::JNIEnv;
use jni::objects::{JClass, JList, JObject, JValue};
use jni::sys::{jbyteArray, jint, jobject};
use jni::errors::{Result as JniResult};
use verifiable::GenerateVerifiable;
use verifiable::ring::bandersnatch::BandersnatchVrfVerifiable;
use verifiable::ring::RingDomainSize;
use std::ptr;

macro_rules! r#try_or_throw {
    ($jni_env: ident, $expr:expr, $ret: expr) => {
        match $expr {
            JniResult::Ok(val) => val,
            JniResult::Err(err) => {
                $jni_env.throw_new("java/lang/Exception", err.description()).unwrap();
                return $ret;
            }
        }
    };
    ($expr:expr,) => {
        $crate::r#try!($expr)
    };
}

macro_rules! r#try_or_throw_null {
    ($jni_env: ident, $expr:expr) => {
        try_or_throw!($jni_env, $expr, ptr::null_mut())
    }
}

fn domain_size_from_ordinal(ordinal: jint) -> Result<RingDomainSize, &'static str> {
    match ordinal {
        0 => Ok(RingDomainSize::Domain11),
        1 => Ok(RingDomainSize::Domain12),
        2 => Ok(RingDomainSize::Domain16),
        _ => Err("Invalid domain size ordinal. Expected 0 (Domain11), 1 (Domain12), or 2 (Domain16)"),
    }
}

#[no_mangle]
fn Java_io_paritytech_polkadotapp_bandersnatch_1crypto_BandersnatchCrypto_derive_1member_1key(
    jni_env: JNIEnv,
    _: JClass,
    entropy: jbyteArray
) -> jbyteArray {
    let entropy_arr = try_or_throw_null!(jni_env, jni_env.convert_byte_array(entropy));
    let entropy_slice: [u8;32] = entropy_arr.try_into().expect("slice with incorrect length");

    let secret = BandersnatchVrfVerifiable::new_secret(entropy_slice);
    let public = BandersnatchVrfVerifiable::member_from_secret(&secret);

    try_or_throw_null!(jni_env, jni_env.byte_array_from_slice(public.as_ref()))
}

#[no_mangle]
fn Java_io_paritytech_polkadotapp_bandersnatch_1crypto_BandersnatchCrypto_sign(
    jni_env: JNIEnv,
    _: JClass,
    entropy: jbyteArray,
    message: jbyteArray,
) -> jbyteArray {
    let entropy = try_or_throw_null!(jni_env, jni_env.convert_byte_array(entropy));
    let entropy: [u8;32] = entropy.try_into().expect("slice with incorrect length");

    let message = try_or_throw_null!(jni_env, jni_env.convert_byte_array(message));

    let secret = BandersnatchVrfVerifiable::new_secret(entropy);
    let signature = BandersnatchVrfVerifiable::sign(&secret, &message).unwrap();

    try_or_throw_null!(jni_env, jni_env.byte_array_from_slice(signature.as_ref()))
}

#[no_mangle]
fn Java_io_paritytech_polkadotapp_bandersnatch_1crypto_BandersnatchCrypto_create_1proof(
    jni_env: JNIEnv,
    _: JClass,
    entropy: jbyteArray,
    members: JObject,
    context: jbyteArray,
    message: jbyteArray,
    domain_size_ordinal: jint,
) -> jobject {
    let domain_size = match domain_size_from_ordinal(domain_size_ordinal) {
        Ok(ds) => ds,
        Err(msg) => {
            jni_env.throw_new("java/lang/IllegalArgumentException", msg).unwrap();
            return ptr::null_mut();
        }
    };
    let capacity = domain_size.into();

    let entropy_slice: [u8;32] = try_or_throw_null!(jni_env, jni_env.convert_byte_array(entropy))
        .try_into()
        .expect("slice with incorrect length");

    let secret = BandersnatchVrfVerifiable::new_secret(entropy_slice);
    let public = BandersnatchVrfVerifiable::member_from_secret(&secret);

    let context_slice = try_or_throw_null!(jni_env, jni_env.convert_byte_array(context));
    let message_slice = try_or_throw_null!(jni_env, jni_env.convert_byte_array(message));

    let members_list: JList = try_or_throw_null!(jni_env, jni_env.get_list(members));
    let members_vec = members_to_vec(&jni_env, members_list);

    let commitment = BandersnatchVrfVerifiable::open(capacity, &public, members_vec.into_iter()).unwrap();
    let (proof, alias) = BandersnatchVrfVerifiable::create(
        commitment,
        &secret,
        &context_slice,
        &message_slice,
    )
    .unwrap();

    let proof_bytes = try_or_throw_null!(jni_env, jni_env.byte_array_from_slice(proof.as_ref()));
    let alias_bytes = try_or_throw_null!(jni_env, jni_env.byte_array_from_slice(alias.as_ref()));

    let result_class = try_or_throw_null!(
        jni_env,
        jni_env.find_class("io/paritytech/polkadotapp/bandersnatch_crypto/CreateProofResult")
    );
    let result = try_or_throw_null!(
        jni_env,
        jni_env.new_object(
            result_class,
            "([B[B)V",
            &[
                JValue::Object(JObject::from(proof_bytes)),
                JValue::Object(JObject::from(alias_bytes)),
            ],
        )
    );

    result.into_inner()
}


fn members_to_vec(jni_env: &JNIEnv, members: JList) -> Vec<[u8; 32]> {
    let size = members.size().expect("Failed to get list size");

    (0..size).map(|i|{
        let itemObj = members.get(i).expect("Failed to get item").unwrap();
        let itemArr: jbyteArray = itemObj.cast();
        let itemSlice: [u8; 32] = jni_env
        .convert_byte_array(itemArr)
        .expect("Failed to convert byte array")
        .try_into()
        .expect("slice with incorrect length");

        itemSlice
    }).collect()
}

#[no_mangle]
fn Java_io_paritytech_polkadotapp_bandersnatch_1crypto_BandersnatchCrypto_alias_1in_1context(
    jni_env: JNIEnv,
    _: JClass,
    entropy: jbyteArray,
    context: jbyteArray,
) -> jbyteArray {
    let entropy_slice: [u8; 32] = try_or_throw_null!(jni_env, jni_env.convert_byte_array(entropy))
        .try_into()
        .expect("slice with incorrect length");

    let context_slice = try_or_throw_null!(jni_env, jni_env.convert_byte_array(context));

    let secret = BandersnatchVrfVerifiable::new_secret(entropy_slice);

    let alias_result = BandersnatchVrfVerifiable::alias_in_context(&secret, &context_slice);

    let alias = match alias_result {
        Ok(a) => a,
        Err(_) => {
            jni_env.throw_new("java/lang/Exception", "Failed to create alias").unwrap();
            return ptr::null_mut();
        }
    };

    try_or_throw_null!(jni_env, jni_env.byte_array_from_slice(alias.as_ref()))
}
