use anyhow::{Context, Result};
use pgp::composed::{Message, SignedPublicKey};
use pgp::packet::Signature;
use pgp::types::KeyId;
use std::io::Cursor;
use thiserror::Error;

#[derive(Clone, Debug, Error)]
#[error("Message was not the correct type. Expected signed.")]
struct MessageNotSigned;

#[derive(Clone, Debug, Error)]
#[error("Message had the wrong number of issues. Expected one, got {0:?}")]
struct MessageBadIssuers(Vec<KeyId>);

pub fn parse_message<'a>(message: &'a [u8]) -> Result<(Signature, Vec<u8>)> {
    let mut message = Message::from_bytes(Cursor::new(message))?;

    let data = message.as_data_vec()?;

    let signature = if let Message::Signed { reader, .. } = message {
        reader.signature().clone()
    } else if let Message::SignedOnePass { reader, .. } = message {
        reader
            .signature()
            .ok_or(MessageNotSigned)
            .with_context(|| "Message was SignedOnePass but missing signature packet.")?
            .clone()
    } else {
        return Err(MessageNotSigned.into());
    };

    Ok((signature, data))
}

pub fn message_keyid<'a>(sig: &Signature) -> Result<KeyId> {
    let issuers = sig.issuer();
    if let [id] = issuers.as_slice() {
        Ok((*id).clone())
    } else {
        Err(MessageBadIssuers(issuers.into_iter().cloned().collect()).into())
    }
}

pub fn verify_message(signature: &Signature, key: &SignedPublicKey, data: &[u8]) -> Result<()> {
    signature.verify(key, data)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use pgp::types::KeyDetails;
    use rand::thread_rng;

    use pgp::composed::{Deserializable, MessageBuilder, SignedPublicKey, SignedSecretKey};
    use pgp::crypto::hash::HashAlgorithm;
    use pgp::types::Password;
    use std::{fs, io::Cursor, path::Path};

    use super::*;

    fn read_skey_file(path: impl AsRef<Path>) -> Result<SignedSecretKey> {
        let bytes = fs::read(path.as_ref())
            .with_context(|| format!("Failed to read pgp secret key at {:?}", path.as_ref()))?;

        let (skey, _) = SignedSecretKey::from_armor_single_buf(Cursor::new(bytes))
            .with_context(|| format!("Failed to parse pgp secret key at {:?}", path.as_ref()))?;

        Ok(skey)
    }

    fn read_pkey_file(path: impl AsRef<Path>) -> Result<SignedPublicKey> {
        let bytes = fs::read(path.as_ref())
            .with_context(|| format!("Failed to read pgp public key at {:?}", path.as_ref()))?;
        let (pkey, _) = SignedPublicKey::from_armor_single_buf(Cursor::new(bytes))
            .with_context(|| format!("Failed to parse pgp public key at {:?}", path.as_ref()))?;
        Ok(pkey)
    }

    #[test]
    fn test_sign_verify() -> Result<()> {
        let pkey =
            read_pkey_file("test.asc").with_context(|| "Must create a file called test.asc")?;
        let skey = read_skey_file("test_secret.asc")
            .with_context(|| "Must create a file called test_secret.asc")?;

        let plaintext = b"hello world";
        let hash_alg = HashAlgorithm::Sha256;

        let mut builder = MessageBuilder::from_bytes("", plaintext.to_vec());

        builder.sign(&skey.primary_key, Password::empty(), hash_alg);
        let signed_text = builder.to_vec(thread_rng())?;

        let (sig, data) = parse_message(&signed_text)?;
        let key_id = message_keyid(&sig)?;

        assert_eq!(key_id, skey.key_id());
        verify_message(&sig, &pkey, &data)?;

        assert_eq!(data, plaintext);
        Ok(())
    }
}
