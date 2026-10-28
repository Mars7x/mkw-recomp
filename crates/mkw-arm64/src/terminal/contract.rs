//! Immutable source contract for a separate guarded execution profile. This
//! binds a theorem to source, never authenticates arbitrary native machine code.
use super::{
    conditional::{model_fingerprint, Analysis, Effect, Kind},
    Image, SourceStamp, TerminalSite,
};
use crate::{Instruction, Op};
use sha2::{Digest, Sha256};
use std::sync::Arc;
struct Data {
    image: Image,
    source: SourceStamp,
    ins: Vec<Instruction>,
    sites: Vec<TerminalSite>,
    effects: Vec<Effect>,
    claim: Option<Effect>,
    next: Option<Effect>,
    digest: [u8; 32],
}
#[derive(Clone)]
pub struct Contract {
    data: Arc<Data>,
}
impl Contract {
    pub fn analyze(
        image: &Image,
        entry: u64,
        bytes: usize,
        effects: &[Effect],
    ) -> Result<Self, String> {
        let analysis = image
            .analyze_conditionally(entry, bytes, effects)
            .map_err(|e| format!("guarded source:{e:?}"))?;
        Self::control(image, analysis)
    }
    fn control(image: &Image, a: Analysis) -> Result<Self, String> {
        if !a.is_closed_conditionally() {
            return Err("guarded source graph is not conditionally closed".into());
        }
        let claim = a
            .no_return_effect()
            .map_err(|e| format!("guarded theorem:{e:?}"))?;
        Self::build(
            image,
            a.source(),
            a.instructions().to_vec(),
            a.terminal_sites().to_vec(),
            a.dependencies().to_vec(),
            claim,
            None,
        )
    }
    /// Exact theorem-backed provider. Initial Local support is deliberately bounded
    /// to independently closed sources with no local supporting-fact DAG.
    pub fn effect(image: &Image, effect: &Effect) -> Result<Self, String> {
        if !effect.belongs_to(image) {
            return Err("foreign guarded theorem".into());
        }
        match effect.kind() {
            Kind::Local => {
                let fact = effect.local_source_fact().ok_or("missing local evidence")?;
                if !fact.supporting_facts().is_empty() {
                    return Err("guarded local supporting facts not implemented".into());
                }
                let s = fact.source();
                let actual = image
                    .analyze(s.entry, s.bytes, &[])
                    .map_err(|e| format!("guarded seed:{e:?}"))?
                    .no_return_fact()
                    .ok_or("guarded seed returns")?;
                if Effect::from_local(&actual).digest() != effect.digest() {
                    return Err("guarded seed differs".into());
                }
                let a = image
                    .analyze_conditionally(s.entry, s.bytes, &[])
                    .map_err(|e| format!("seed control:{e:?}"))?;
                Self::build(
                    image,
                    s,
                    a.instructions().to_vec(),
                    Vec::new(),
                    Vec::new(),
                    Some(effect.clone()),
                    None,
                )
            }
            Kind::Control => {
                let s = effect.source();
                let c = Self::analyze(image, s.entry, s.bytes, effect.dependencies())?;
                if !c.matches(effect) {
                    return Err("guarded derivation differs".into());
                }
                Ok(c)
            }
            Kind::Import => {
                let requirement = effect
                    .requirements()
                    .iter()
                    .find(|r| r.thunk().entry() == effect.source().entry)
                    .ok_or("missing direct import requirement")?;
                if requirement.thunk().sources().len() != 1 {
                    return Err("guarded veneer fragments not implemented".into());
                }
                let provider = effect
                    .dependencies()
                    .first()
                    .ok_or("missing import provider")?;
                if image
                    .conditional_import(requirement.thunk(), provider)
                    .map_err(|e| format!("guarded import:{e:?}"))?
                    .digest()
                    != effect.digest()
                {
                    return Err("guarded import derivation differs".into());
                }
                let s = effect.source();
                let a = image
                    .analyze(s.entry, s.bytes, &[])
                    .map_err(|e| format!("guarded import source:{e:?}"))?;
                Self::build(
                    image,
                    s,
                    a.instructions().to_vec(),
                    Vec::new(),
                    Vec::new(),
                    Some(effect.clone()),
                    Some(provider.clone()),
                )
            }
        }
    }
    /// Ordinary source-verified service/LLSC body, without a no-return claim. This
    /// does not widen the default conditional theorem's instruction model.
    pub fn plain_services(image: &Image, entry: u64, bytes: usize) -> Result<Self, String> {
        let stamp = image
            .analyze(entry, bytes, &[])
            .map_err(|e| format!("plain source:{e:?}"))?
            .source();
        let off = (entry - image.data.base) as usize;
        let raw = &image.data.bytes[off..off + bytes];
        let ins = crate::lift_function_services_v3(raw, entry, entry)?;
        Self::build(image, stamp, ins, Vec::new(), Vec::new(), None, None)
    }
    fn build(
        image: &Image,
        source: SourceStamp,
        mut ins: Vec<Instruction>,
        sites: Vec<TerminalSite>,
        effects: Vec<Effect>,
        claim: Option<Effect>,
        next: Option<Effect>,
    ) -> Result<Self, String> {
        let pcs = ins
            .iter()
            .map(|i| i.pc)
            .collect::<std::collections::BTreeSet<_>>();
        for i in &mut ins {
            if let Op::Branch { target } = i.op {
                if !pcs.contains(&target) {
                    i.op = Op::TailBranch { target };
                }
            }
        }
        let mut h = Sha256::new();
        h.update(b"mkw-source-contract-v4\0");
        h.update(model_fingerprint());
        h.update(source.image_digest);
        h.update(source.text_base.to_le_bytes());
        h.update(source.entry.to_le_bytes());
        h.update((source.bytes as u64).to_le_bytes());
        h.update(source.body_digest);
        h.update([u8::from(claim.is_some()), u8::from(next.is_some())]);
        if let Some(c) = &claim {
            h.update(c.digest());
        }
        if let Some(n) = &next {
            h.update(n.digest());
        }
        h.update((effects.len() as u64).to_le_bytes());
        for e in &effects {
            h.update(e.digest());
        }
        Ok(Self {
            data: Arc::new(Data {
                image: image.clone(),
                source,
                ins,
                sites,
                effects,
                claim,
                next,
                digest: h.finalize().into(),
            }),
        })
    }
    pub fn source(&self) -> SourceStamp {
        self.data.source
    }
    pub fn instructions(&self) -> &[Instruction] {
        &self.data.ins
    }
    pub fn terminal_sites(&self) -> &[TerminalSite] {
        &self.data.sites
    }
    pub fn effects(&self) -> &[Effect] {
        &self.data.effects
    }
    pub fn claim(&self) -> Option<&Effect> {
        self.data.claim.as_ref()
    }
    pub fn alias_next(&self) -> Option<&Effect> {
        self.data.next.as_ref()
    }
    pub fn matches(&self, e: &Effect) -> bool {
        e.belongs_to(&self.data.image) && self.claim().is_some_and(|c| c.digest() == e.digest())
    }
    pub fn digest(&self) -> [u8; 32] {
        self.data.digest
    }
    /// Producer identity must come from the trusted build. This commitment/query
    /// detects mismatches; it is not a proof about arbitrary native machine code.
    pub fn origin(&self, producer: [u8; 32]) -> [u8; 32] {
        let mut h = Sha256::new();
        h.update(b"mkw-guarded-origin-v4\0");
        h.update(self.digest());
        h.update(producer);
        h.finalize().into()
    }
}
