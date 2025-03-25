use static_btree::key::{KeyEncoder, KeyEncoderFactory, KeyType};
use std::cmp::Ordering;

#[test]
fn test_i64_key_encoder() {
    let encoder = KeyEncoderFactory::i64();

    // Test encoding
    let key = 42i64;
    let encoded = encoder.encode(&key).unwrap();
    assert_eq!(encoded.len(), 8);
    assert_eq!(encoded, key.to_le_bytes());

    // Test decoding
    let decoded = encoder.decode(&encoded).unwrap();
    assert_eq!(decoded, key);

    // Test comparison
    let key1 = 10i64;
    let key2 = 20i64;
    let encoded1 = encoder.encode(&key1).unwrap();
    let encoded2 = encoder.encode(&key2).unwrap();

    assert_eq!(encoder.compare(&encoded1, &encoded2), Ordering::Less);
    assert_eq!(encoder.compare(&encoded2, &encoded1), Ordering::Greater);
    assert_eq!(encoder.compare(&encoded1, &encoded1), Ordering::Equal);
}

#[test]
fn test_i32_key_encoder() {
    let encoder = KeyEncoderFactory::i32();

    // Test encoding
    let key = 42i32;
    let encoded = encoder.encode(&key).unwrap();
    assert_eq!(encoded.len(), 4);
    assert_eq!(encoded, key.to_le_bytes());

    // Test decoding
    let decoded = encoder.decode(&encoded).unwrap();
    assert_eq!(decoded, key);

    // Test comparison
    let key1 = 10i32;
    let key2 = 20i32;
    let encoded1 = encoder.encode(&key1).unwrap();
    let encoded2 = encoder.encode(&key2).unwrap();

    assert_eq!(encoder.compare(&encoded1, &encoded2), Ordering::Less);
    assert_eq!(encoder.compare(&encoded2, &encoded1), Ordering::Greater);
    assert_eq!(encoder.compare(&encoded1, &encoded1), Ordering::Equal);
}

#[test]
fn test_i16_key_encoder() {
    let encoder = KeyEncoderFactory::i16();

    // Test encoding
    let key = 42i16;
    let encoded = encoder.encode(&key).unwrap();
    assert_eq!(encoded.len(), 2);
    assert_eq!(encoded, key.to_le_bytes());

    // Test decoding
    let decoded = encoder.decode(&encoded).unwrap();
    assert_eq!(decoded, key);
}

#[test]
fn test_i8_key_encoder() {
    let encoder = KeyEncoderFactory::i8();

    // Test encoding
    let key = 42i8;
    let encoded = encoder.encode(&key).unwrap();
    assert_eq!(encoded.len(), 1);
    assert_eq!(encoded[0], key as u8);

    // Test decoding
    let decoded = encoder.decode(&encoded).unwrap();
    assert_eq!(decoded, key);
}

#[test]
fn test_u64_key_encoder() {
    let encoder = KeyEncoderFactory::u64();

    // Test encoding
    let key = 42u64;
    let encoded = encoder.encode(&key).unwrap();
    assert_eq!(encoded.len(), 8);
    assert_eq!(encoded, key.to_le_bytes());

    // Test decoding
    let decoded = encoder.decode(&encoded).unwrap();
    assert_eq!(decoded, key);
}

#[test]
fn test_u32_key_encoder() {
    let encoder = KeyEncoderFactory::u32();

    // Test encoding
    let key = 42u32;
    let encoded = encoder.encode(&key).unwrap();
    assert_eq!(encoded.len(), 4);
    assert_eq!(encoded, key.to_le_bytes());

    // Test decoding
    let decoded = encoder.decode(&encoded).unwrap();
    assert_eq!(decoded, key);
}

#[test]
fn test_u16_key_encoder() {
    let encoder = KeyEncoderFactory::u16();

    // Test encoding
    let key = 42u16;
    let encoded = encoder.encode(&key).unwrap();
    assert_eq!(encoded.len(), 2);
    assert_eq!(encoded, key.to_le_bytes());

    // Test decoding
    let decoded = encoder.decode(&encoded).unwrap();
    assert_eq!(decoded, key);
}

#[test]
fn test_u8_key_encoder() {
    let encoder = KeyEncoderFactory::u8();

    // Test encoding
    let key = 42u8;
    let encoded = encoder.encode(&key).unwrap();
    assert_eq!(encoded.len(), 1);
    assert_eq!(encoded[0], key);

    // Test decoding
    let decoded = encoder.decode(&encoded).unwrap();
    assert_eq!(decoded, key);
}

#[test]
fn test_error_handling() {
    let encoder = KeyEncoderFactory::i64();

    // Test decoding with too small buffer
    let too_small = vec![1, 2, 3];
    let result = encoder.decode(&too_small);
    assert!(result.is_err());
}

#[test]
fn test_factory_encoder_creation() {
    // Test creating encoders from the factory for different types
    let i8_encoder = KeyEncoderFactory::for_type::<i8>(KeyType::I8);
    let i16_encoder = KeyEncoderFactory::for_type::<i16>(KeyType::I16);
    let i32_encoder = KeyEncoderFactory::for_type::<i32>(KeyType::I32);
    let i64_encoder = KeyEncoderFactory::for_type::<i64>(KeyType::I64);

    assert_eq!(i8_encoder.encoded_size(), 1);
    assert_eq!(i16_encoder.encoded_size(), 2);
    assert_eq!(i32_encoder.encoded_size(), 4);
    assert_eq!(i64_encoder.encoded_size(), 8);
}
