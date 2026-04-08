//! Binary serialization for WASM ledger caching.
//!
//! Provides rkyv-based serialization of parsed ledgers, enabling storage in
//! browser OPFS or `IndexedDB` and fast cache restores without re-parsing and
//! re-booking.
//!
//! # Cache format
//!
//! Each cache blob starts with a 12-byte header (8-byte magic + 4-byte version)
//! followed by an rkyv-serialized payload.
//!
//! # Cache invalidation
//!
//! Use [`hash_sources`] to compute a SHA-256 fingerprint of the source files.
//! Store the fingerprint alongside the cache bytes and compare on load; if the
//! fingerprint changed, discard the cache and re-parse.

use rustledger_core::Directive;

use crate::types::{Error, LedgerOptions};

/// Current cache format version. Increment when the serialized format changes.
pub const CACHE_VERSION: u32 = 1;

/// Magic bytes prepended to every cache blob.
pub const CACHE_MAGIC: &[u8; 8] = b"WLEDGER\0";

/// Header size: 8 (magic) + 4 (version).
const HEADER_SIZE: usize = 12;

// =============================================================================
// Payload types
// =============================================================================

/// Cache payload for a [`super::parsed_ledger::ParsedLedger`].
#[derive(Debug, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct ParsedLedgerPayload {
    pub directives: Vec<Directive>,
    pub options: LedgerOptions,
    pub parse_errors: Vec<Error>,
    pub validation_errors: Vec<Error>,
}

/// Cache payload for a [`super::parsed_ledger::Ledger`].
#[derive(Debug, rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
pub struct LedgerPayload {
    pub directives: Vec<Directive>,
    pub options: LedgerOptions,
    pub errors: Vec<Error>,
}

// =============================================================================
// Encode / decode
// =============================================================================

/// Validate and strip the cache header, returning the payload data slice.
fn strip_header(bytes: &[u8]) -> Result<&[u8], String> {
    if bytes.len() < HEADER_SIZE {
        return Err("Invalid cache: data too short".to_string());
    }
    let (header, data) = bytes.split_at(HEADER_SIZE);
    if &header[..8] != CACHE_MAGIC {
        return Err("Invalid cache: unrecognized magic bytes".to_string());
    }
    let version = u32::from_le_bytes(header[8..12].try_into().unwrap());
    if version != CACHE_VERSION {
        return Err(format!(
            "Cache version mismatch: expected {CACHE_VERSION}, got {version}. Re-parse the ledger."
        ));
    }
    Ok(data)
}

/// Prepend the cache header to rkyv-serialized data.
fn prepend_header(data: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(HEADER_SIZE + data.len());
    result.extend_from_slice(CACHE_MAGIC);
    result.extend_from_slice(&CACHE_VERSION.to_le_bytes());
    result.extend_from_slice(data);
    result
}

/// Serialize a [`ParsedLedgerPayload`] to bytes.
pub fn serialize_parsed(payload: &ParsedLedgerPayload) -> Result<Vec<u8>, String> {
    let data = rkyv::to_bytes::<rkyv::rancor::Error>(payload)
        .map_err(|e| format!("Serialization failed: {e}"))?;
    Ok(prepend_header(&data))
}

/// Deserialize a [`ParsedLedgerPayload`] from bytes.
pub fn deserialize_parsed(bytes: &[u8]) -> Result<ParsedLedgerPayload, String> {
    let data = strip_header(bytes)?;
    rkyv::from_bytes::<ParsedLedgerPayload, rkyv::rancor::Error>(data)
        .map_err(|e| format!("Deserialization failed: {e}"))
}

/// Serialize a [`LedgerPayload`] to bytes.
pub fn serialize_ledger(payload: &LedgerPayload) -> Result<Vec<u8>, String> {
    let data = rkyv::to_bytes::<rkyv::rancor::Error>(payload)
        .map_err(|e| format!("Serialization failed: {e}"))?;
    Ok(prepend_header(&data))
}

/// Deserialize a [`LedgerPayload`] from bytes.
pub fn deserialize_ledger(bytes: &[u8]) -> Result<LedgerPayload, String> {
    let data = strip_header(bytes)?;
    rkyv::from_bytes::<LedgerPayload, rkyv::rancor::Error>(data)
        .map_err(|e| format!("Deserialization failed: {e}"))
}

// =============================================================================
// Source fingerprinting
// =============================================================================

/// Compute a SHA-256 fingerprint of one or more source strings.
///
/// Returns the hash as a lowercase hex string. Store this alongside cached
/// bytes and compare on the next load; if the fingerprint changed, discard
/// the cache.
///
/// Sources are separated by NUL bytes so `["ab", "c"]` differs from `["a", "bc"]`.
pub fn hash_sources(sources: &[&str]) -> String {
    use sha2::{Digest, Sha256};
    use std::fmt::Write as _;

    let mut hasher = Sha256::new();
    for source in sources {
        hasher.update(source.as_bytes());
        hasher.update(b"\x00");
    }
    let result = hasher.finalize();
    result.iter().fold(String::with_capacity(64), |mut acc, b| {
        let _ = write!(acc, "{b:02x}");
        acc
    })
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_roundtrip_ledger_payload() {
        let payload = LedgerPayload {
            directives: Vec::new(),
            options: LedgerOptions {
                operating_currencies: vec!["USD".to_string()],
                title: Some("Test".to_string()),
            },
            errors: vec![Error::new("a warning")],
        };
        let bytes = serialize_ledger(&payload).expect("serialize");
        assert!(bytes.starts_with(CACHE_MAGIC));

        let restored = deserialize_ledger(&bytes).expect("deserialize");
        assert_eq!(restored.options.operating_currencies, ["USD"]);
        assert_eq!(restored.options.title.as_deref(), Some("Test"));
        assert_eq!(restored.errors.len(), 1);
    }

    #[test]
    fn test_roundtrip_with_directives() {
        use crate::helpers::load_and_book;

        let source = r#"
option "title" "Test"
option "operating_currency" "USD"

2024-01-01 open Assets:Bank USD
2024-01-01 open Expenses:Food USD

2024-01-15 * "Coffee"
  Expenses:Food  5.00 USD
  Assets:Bank   -5.00 USD
"#;
        let processed = load_and_book(source);
        assert!(!processed.directives.is_empty());

        let payload = ParsedLedgerPayload {
            directives: processed.directives.clone(),
            options: processed.options.clone(),
            parse_errors: Vec::new(),
            validation_errors: Vec::new(),
        };

        let bytes = serialize_parsed(&payload).expect("serialize");
        let restored = deserialize_parsed(&bytes).expect("deserialize");
        assert_eq!(restored.directives.len(), processed.directives.len());
        assert_eq!(restored.options.title.as_deref(), Some("Test"));
    }

    #[test]
    fn test_bad_magic_returns_error() {
        let mut bytes = serialize_ledger(&LedgerPayload {
            directives: Vec::new(),
            options: LedgerOptions::default(),
            errors: Vec::new(),
        })
        .unwrap();
        bytes[0] = b'X';
        assert!(deserialize_ledger(&bytes).unwrap_err().contains("magic"));
    }

    #[test]
    fn test_too_short_returns_error() {
        assert!(
            deserialize_ledger(b"short")
                .unwrap_err()
                .contains("too short")
        );
    }

    #[test]
    fn test_version_mismatch_returns_error() {
        let mut bytes = serialize_ledger(&LedgerPayload {
            directives: Vec::new(),
            options: LedgerOptions::default(),
            errors: Vec::new(),
        })
        .unwrap();
        bytes[8..12].copy_from_slice(&99u32.to_le_bytes());
        assert!(
            deserialize_ledger(&bytes)
                .unwrap_err()
                .contains("version mismatch")
        );
    }

    #[test]
    fn test_hash_sources_deterministic() {
        let h1 = hash_sources(&["hello", "world"]);
        let h2 = hash_sources(&["hello", "world"]);
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 64);
    }

    #[test]
    fn test_hash_sources_distinguishes_concat() {
        let h1 = hash_sources(&["ab", "c"]);
        let h2 = hash_sources(&["a", "bc"]);
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_hash_sources_changes_with_content() {
        let h1 = hash_sources(&["source v1"]);
        let h2 = hash_sources(&["source v2"]);
        assert_ne!(h1, h2);
    }
}
