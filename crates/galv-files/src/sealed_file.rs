//! Versioned, recipient-encrypted air-gap file artifacts.
//!
//! Uses the same hybrid encryption as High/Low database bundles: a fresh
//! XChaCha20-Poly1305 key wrapped for the High recipient with RSA-OAEP/SHA-256.
use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    XChaCha20Poly1305, XNonce,
};
use getrandom::fill;
use rsa::{
    pkcs8::{DecodePrivateKey, DecodePublicKey},
    Oaep, RsaPrivateKey, RsaPublicKey,
};
use sha2::Sha256;

use crate::{validate_file_uuid, FileError};

const MAGIC: &[u8] = b"GALVFILE\x01";
const NONCE_LEN: usize = 24;
const TAG_LEN: usize = 16;

fn aad(uuid: &str) -> Vec<u8> {
    format!("galvanize-airgap-file/1\n{uuid}").into_bytes()
}

/// Seal plaintext for the High domain. The local database key is not used.
pub fn seal_file(
    uuid: &str,
    plaintext: &[u8],
    recipient_public_key_pem: &str,
) -> Result<Vec<u8>, FileError> {
    validate_file_uuid(uuid)?;
    let recipient = RsaPublicKey::from_public_key_pem(recipient_public_key_pem).map_err(|e| {
        FileError::EncryptionFailed(format!("invalid recipient RSA public key: {e}"))
    })?;
    let mut key = [0_u8; 32];
    let mut nonce = [0_u8; NONCE_LEN];
    fill(&mut key)
        .map_err(|e| FileError::EncryptionFailed(format!("random key generation failed: {e}")))?;
    fill(&mut nonce)
        .map_err(|e| FileError::EncryptionFailed(format!("random nonce generation failed: {e}")))?;
    let wrapped = recipient
        .encrypt(&mut rsa::rand_core::OsRng, Oaep::new::<Sha256>(), &key)
        .map_err(|e| FileError::EncryptionFailed(format!("could not wrap file key: {e}")))?;
    let wrapped_len = u16::try_from(wrapped.len())
        .map_err(|_| FileError::EncryptionFailed("wrapped file key is too large".into()))?;
    let cipher = XChaCha20Poly1305::new_from_slice(&key)
        .map_err(|e| FileError::EncryptionFailed(e.to_string()))?;
    let ciphertext = cipher
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: plaintext,
                aad: &aad(uuid),
            },
        )
        .map_err(|e| FileError::EncryptionFailed(e.to_string()))?;
    let mut out =
        Vec::with_capacity(MAGIC.len() + 2 + wrapped.len() + NONCE_LEN + ciphertext.len());
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&wrapped_len.to_be_bytes());
    out.extend_from_slice(&wrapped);
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

/// Open a sealed artifact; rejects legacy plaintext and a different UUID.
pub fn open_file(
    uuid: &str,
    artifact: &[u8],
    recipient_private_key_pem: &str,
) -> Result<Vec<u8>, FileError> {
    validate_file_uuid(uuid)?;
    if !artifact.starts_with(MAGIC) || artifact.len() < MAGIC.len() + 2 {
        return Err(FileError::DecryptionFailed);
    }
    let key_len = u16::from_be_bytes([artifact[MAGIC.len()], artifact[MAGIC.len() + 1]]) as usize;
    let wrapped_end = MAGIC.len() + 2 + key_len;
    if key_len == 0 || artifact.len() < wrapped_end + NONCE_LEN + TAG_LEN {
        return Err(FileError::DecryptionFailed);
    }
    let private = RsaPrivateKey::from_pkcs8_pem(recipient_private_key_pem)
        .map_err(|_| FileError::DecryptionFailed)?;
    let key = private
        .decrypt(
            Oaep::new::<Sha256>(),
            &artifact[MAGIC.len() + 2..wrapped_end],
        )
        .map_err(|_| FileError::DecryptionFailed)?;
    let cipher =
        XChaCha20Poly1305::new_from_slice(&key).map_err(|_| FileError::DecryptionFailed)?;
    let nonce = XNonce::from_slice(&artifact[wrapped_end..wrapped_end + NONCE_LEN]);
    cipher
        .decrypt(
            nonce,
            Payload {
                msg: &artifact[wrapped_end + NONCE_LEN..],
                aad: &aad(uuid),
            },
        )
        .map_err(|_| FileError::DecryptionFailed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rsa::pkcs8::{EncodePrivateKey, EncodePublicKey, LineEnding};

    const UUID: &str = "f0000000-1111-2222-3333-444455556666";

    #[test]
    fn sealed_file_roundtrip_and_rejects_tampering_or_plaintext() {
        let private = RsaPrivateKey::new(&mut rsa::rand_core::OsRng, 2048).unwrap();
        let public = private
            .to_public_key()
            .to_public_key_pem(LineEnding::LF)
            .unwrap();
        let private_pem = private.to_pkcs8_pem(LineEnding::LF).unwrap();
        let plain = b"classified document";
        let mut sealed = seal_file(UUID, plain, &public).unwrap();
        assert!(!sealed.windows(plain.len()).any(|window| window == plain));
        assert_eq!(open_file(UUID, &sealed, &private_pem).unwrap(), plain);
        assert!(open_file(
            "f0000000-1111-2222-3333-444455556667",
            &sealed,
            &private_pem
        )
        .is_err());
        assert!(open_file(UUID, plain, &private_pem).is_err());
        let other_private = RsaPrivateKey::new(&mut rsa::rand_core::OsRng, 2048).unwrap();
        let other_private_pem = other_private.to_pkcs8_pem(LineEnding::LF).unwrap();
        assert!(open_file(UUID, &sealed, &other_private_pem).is_err());
        *sealed.last_mut().unwrap() ^= 1;
        assert!(open_file(UUID, &sealed, &private_pem).is_err());
    }
}
