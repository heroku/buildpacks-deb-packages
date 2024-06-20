use sequoia_openpgp::parse::stream::{MessageLayer, MessageStructure, VerificationHelper};
use sequoia_openpgp::{Cert, KeyHandle, Result};

pub(crate) struct CertHelper {
    cert: Cert,
}

impl CertHelper {
    pub(crate) fn new(cert: Cert) -> CertHelper {
        CertHelper { cert }
    }
}

// This was adapted from the example verification process detailed at:
// https://gitlab.com/sequoia-pgp/sequoia/-/blob/main/openpgp/examples/generate-sign-verify.rs
impl VerificationHelper for CertHelper {
    fn get_certs(&mut self, _: &[KeyHandle]) -> Result<Vec<Cert>> {
        Ok(vec![self.cert.clone()])
    }

    fn check(&mut self, structure: MessageStructure) -> Result<()> {
        for (i, layer) in structure.into_iter().enumerate() {
            match (i, layer) {
                // Consider only level 0 signatures (signatures over the data)
                (0, MessageLayer::SignatureGroup { results }) => {
                    return results
                        .into_iter()
                        .next()
                        .ok_or(anyhow::anyhow!("No signature"))
                        .and_then(|verification_result| {
                            verification_result
                                .map(|_| ())
                                .map_err(|e| sequoia_openpgp::Error::from(e).into())
                        });
                }
                _ => Err(anyhow::anyhow!("Unexpected message structure"))?,
            }
        }
        Err(anyhow::anyhow!("Signature verification failed"))?
    }
}
