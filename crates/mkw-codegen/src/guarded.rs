//! Explicit guarded artifact. Only opaque source contracts reach this renderer.
use mkw_arm64::terminal::contract::Contract;
use sha2::{Digest, Sha256};
use std::sync::OnceLock;
pub struct Artifact {
    contract: Contract,
    code: String,
    origin: [u8; 32],
    producer: [u8; 32],
    name: String,
}
pub(crate) struct Config<'a> {
    pub contract: &'a Contract,
    pub name: &'a str,
    pub origin: [u8; 32],
}
impl Artifact {
    pub fn new(contract: Contract, name: &str) -> Result<Self, String> {
        if !name.starts_with("mkw_gv4_")
            || name.len() > 128
            || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        {
            return Err("guarded entry needs a bounded mkw_gv4_ identifier".into());
        }
        let producer = producer_fingerprint();
        let origin = contract.origin(producer);
        let config = Config {
            contract: &contract,
            name,
            origin,
        };
        let code = super::emit_internal(
            contract.instructions(),
            contract.source().entry,
            true,
            true,
            true,
            Some(&config),
        )?;
        Ok(Self {
            contract,
            code,
            origin,
            producer,
            name: name.into(),
        })
    }
    pub fn code(&self) -> &str {
        &self.code
    }
    pub fn contract(&self) -> &Contract {
        &self.contract
    }
    pub fn origin(&self) -> [u8; 32] {
        self.origin
    }
    pub fn producer(&self) -> [u8; 32] {
        self.producer
    }
    pub fn name(&self) -> &str {
        &self.name
    }
}
pub fn producer_fingerprint() -> [u8; 32] {
    static F: OnceLock<[u8; 32]> = OnceLock::new();
    *F.get_or_init(|| {
        let mut h = Sha256::new();
        h.update(b"mkw-guarded-producer-v4\0");
        for s in mkw_arm64::SOURCE_INPUTS
            .iter()
            .chain(super::SOURCE_INPUTS.iter())
        {
            h.update((s.len() as u64).to_le_bytes());
            h.update(s.as_bytes());
        }
        h.finalize().into()
    })
}
