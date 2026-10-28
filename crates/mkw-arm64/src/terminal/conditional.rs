//! Conditional source evidence only. Runtime obligations are never discharged
//! by this module, and no existing compiler/loader consumes these objects.
use super::{
    import::ImportThunk, Image, Issue, NoReturn, SetupError, SiteKind, SourceStamp, TerminalSite,
};
use crate::{decode, Instruction, Op};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    sync::{Arc, OnceLock},
};
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Source(SetupError),
    ConflictingSlot,
    ConflictingAlias,
    TooManyFacts,
    TooManyRequirements,
    DepthLimit,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Local,
    Import,
    Control,
}
/// An undischarged runtime premise. The source thunk and provider stamp are
/// descriptions, not current slot/code authentication or permission to execute.
#[derive(Clone)]
pub struct Requirement {
    thunk: ImportThunk,
    provider: SourceStamp,
}
impl Requirement {
    pub fn thunk(&self) -> &ImportThunk {
        &self.thunk
    }
    pub fn provider(&self) -> SourceStamp {
        self.provider
    }
}
struct Node {
    image: Image,
    source: SourceStamp,
    kind: Kind,
    digest: [u8; 32],
    requirements: Vec<Requirement>,
    dependencies: Vec<Effect>,
    local: Option<NoReturn>,
    depth: u16,
}
/// Opaque no-normal-return theorem UNDER every retained requirement. It is not
/// NoReturn, cannot be deserialized, and has no conversion to unconditional facts.
#[derive(Clone)]
pub struct Effect {
    node: Arc<Node>,
}
impl Effect {
    pub fn from_local(fact: &NoReturn) -> Self {
        Self::new(
            Image {
                data: fact.evidence.image.clone(),
            },
            fact.source(),
            Kind::Local,
            Vec::new(),
            Vec::new(),
            Some(fact.clone()),
        )
        .unwrap()
    }
    pub fn belongs_to(&self, image: &Image) -> bool {
        Arc::ptr_eq(&self.node.image.data, &image.data)
    }
    pub fn source(&self) -> SourceStamp {
        self.node.source
    }
    pub fn kind(&self) -> Kind {
        self.node.kind
    }
    pub fn digest(&self) -> [u8; 32] {
        self.node.digest
    }
    pub fn requirements(&self) -> &[Requirement] {
        &self.node.requirements
    }
    pub fn dependencies(&self) -> &[Effect] {
        &self.node.dependencies
    }
    /// Only returns the original, already unconditional object for a Local node.
    /// Its existing supporting_facts remain part of that object's derivation.
    pub fn local_source_fact(&self) -> Option<&NoReturn> {
        self.node.local.as_ref()
    }
    fn new(
        image: Image,
        source: SourceStamp,
        kind: Kind,
        requirements: Vec<Requirement>,
        dependencies: Vec<Effect>,
        local: Option<NoReturn>,
    ) -> Result<Self, Error> {
        let depth = dependencies.iter().map(|d| d.node.depth).max().unwrap_or(0) + 1;
        if depth > 128 {
            return Err(Error::DepthLimit);
        }
        let mut h = Sha256::new();
        h.update(b"mkw-conditional-control-v1\0");
        h.update(model_fingerprint());
        h.update([match kind {
            Kind::Local => 0,
            Kind::Import => 1,
            Kind::Control => 2,
        }]);
        stamp_hash(&mut h, source);
        h.update((dependencies.len() as u64).to_le_bytes());
        for d in &dependencies {
            h.update(d.digest());
        }
        if let Some(f) = &local {
            h.update(f.proof_digest());
        }
        h.update((requirements.len() as u64).to_le_bytes());
        for r in &requirements {
            h.update(r.thunk.slot().to_le_bytes());
            stamp_hash(&mut h, r.provider);
            h.update((r.thunk.sources().len() as u64).to_le_bytes());
            for s in r.thunk.sources() {
                stamp_hash(&mut h, *s);
            }
        }
        Ok(Self {
            node: Arc::new(Node {
                image,
                source,
                kind,
                digest: h.finalize().into(),
                requirements,
                dependencies,
                local,
                depth,
            }),
        })
    }
}
fn stamp_hash(h: &mut Sha256, s: SourceStamp) {
    h.update(s.image_digest);
    h.update(s.text_base.to_le_bytes());
    h.update(s.entry.to_le_bytes());
    h.update((s.bytes as u64).to_le_bytes());
    h.update(s.body_digest);
}
/// Fingerprint of this analysis model/decoder source, not a native build or
/// helper/library attestation. Codegen identity must be bound separately later.
pub fn model_fingerprint() -> [u8; 32] {
    static F: OnceLock<[u8; 32]> = OnceLock::new();
    *F.get_or_init(|| {
        let mut h = Sha256::new();
        h.update(b"mkw-conditional-model-inputs-v1\0");
        for s in crate::SOURCE_INPUTS {
            h.update((s.len() as u64).to_le_bytes());
            h.update(s.as_bytes());
        }
        h.finalize().into()
    })
}
fn merge<'a>(
    requirements: impl Iterator<Item = &'a Requirement>,
) -> Result<Vec<Requirement>, Error> {
    let mut aliases = BTreeMap::<u64, Requirement>::new();
    let mut slots = BTreeMap::new();
    for r in requirements {
        if slots
            .insert(r.thunk.slot(), r.provider)
            .is_some_and(|old| old != r.provider)
        {
            return Err(Error::ConflictingSlot);
        }
        if let Some(old) = aliases.get(&r.thunk.entry()) {
            if old.provider != r.provider || old.thunk.sources() != r.thunk.sources() {
                return Err(Error::ConflictingAlias);
            }
        } else {
            aliases.insert(r.thunk.entry(), r.clone());
        }
        if aliases.len() > 64 {
            return Err(Error::TooManyRequirements);
        }
    }
    Ok(aliases.into_values().collect())
}
pub struct Analysis {
    image: Image,
    source: SourceStamp,
    instructions: Vec<Instruction>,
    issues: Vec<Issue>,
    returns: Vec<u64>,
    sites: Vec<TerminalSite>,
    dependencies: Vec<Effect>,
    requirements: Vec<Requirement>,
}
impl Analysis {
    pub fn source(&self) -> SourceStamp {
        self.source
    }
    pub fn instructions(&self) -> &[Instruction] {
        &self.instructions
    }
    pub fn issues(&self) -> &[Issue] {
        &self.issues
    }
    pub fn possible_normal_returns(&self) -> &[u64] {
        &self.returns
    }
    pub fn terminal_sites(&self) -> &[TerminalSite] {
        &self.sites
    }
    pub fn requirements(&self) -> &[Requirement] {
        &self.requirements
    }
    pub fn dependencies(&self) -> &[Effect] {
        &self.dependencies
    }
    /// Closure only under these premises, not compiler admission/runtime readiness.
    pub fn is_closed_conditionally(&self) -> bool {
        self.issues.is_empty()
    }
    pub fn no_return_effect(&self) -> Result<Option<Effect>, Error> {
        if !self.is_closed_conditionally() || !self.returns.is_empty() {
            return Ok(None);
        }
        Effect::new(
            self.image.clone(),
            self.source,
            Kind::Control,
            self.requirements.clone(),
            self.dependencies.clone(),
            None,
        )
        .map(Some)
    }
}
impl Image {
    /// Explicit conditional alias to an exact same-image provider theorem. No slot
    /// is read, no binding is authenticated, and every premise remains retained.
    pub fn conditional_import(
        &self,
        thunk: &ImportThunk,
        provider: &Effect,
    ) -> Result<Effect, Error> {
        if !thunk.belongs_to(self) || !Arc::ptr_eq(&self.data, &provider.node.image.data) {
            return Err(Error::Source(SetupError::ForeignImageFact));
        }
        if thunk.entry() == provider.source().entry {
            return Err(Error::Source(SetupError::RecursiveFact));
        }
        let direct = Requirement {
            thunk: thunk.clone(),
            provider: provider.source(),
        };
        let requirements = merge(
            provider
                .requirements()
                .iter()
                .chain(std::iter::once(&direct)),
        )?;
        Effect::new(
            self.clone(),
            thunk.sources()[0],
            Kind::Import,
            requirements,
            vec![provider.clone()],
            None,
        )
    }
    /// Separate conditional analysis; default Image::analyze/lifters remain intact.
    /// All ordinary calls may return; only exact opaque effects remove continuations.
    pub fn analyze_conditionally(
        &self,
        entry: u64,
        bytes: usize,
        effects: &[Effect],
    ) -> Result<Analysis, Error> {
        if effects.len() > 1024 {
            return Err(Error::TooManyFacts);
        }
        // Reuse the unchanged source/bounds validation and exact source stamp. Its
        // graph result is not used for the conditional theorem below.
        let source = self
            .analyze(entry, bytes, &[])
            .map_err(Error::Source)?
            .source();
        let end = entry + bytes as u64;
        let mut targets = BTreeMap::new();
        for e in effects {
            if !Arc::ptr_eq(&self.data, &e.node.image.data) {
                return Err(Error::Source(SetupError::ForeignImageFact));
            }
            if e.source().entry == entry {
                return Err(Error::Source(SetupError::RecursiveFact));
            }
            if targets.insert(e.source().entry, e).is_some() {
                return Err(Error::Source(SetupError::DuplicateFact));
            }
        }
        let mut pending = VecDeque::from([entry]);
        let mut visited = BTreeSet::new();
        let mut instructions = BTreeMap::new();
        let mut issues = BTreeMap::new();
        let mut returns = BTreeSet::new();
        let mut sites = BTreeMap::new();
        let mut used = BTreeMap::new();
        while let Some(pc) = pending.pop_front() {
            if !visited.insert(pc) {
                continue;
            }
            if pc < entry || pc >= end || pc % 4 != 0 {
                issues.insert(pc, Issue::MissingEdge { pc });
                continue;
            }
            let at = (pc - self.data.base) as usize;
            let word = u32::from_le_bytes(self.data.bytes[at..at + 4].try_into().unwrap());
            let ins = match decode(word, pc) {
                Ok(i) => i,
                Err(message) => {
                    issues.insert(pc, Issue::Decode { pc, word, message });
                    pending.push_back(pc + 4);
                    continue;
                }
            };
            match ins.op {
                Op::Ret { .. } | Op::AuthReturn { .. } | Op::IndirectBranch { .. } => {
                    returns.insert(pc);
                }
                Op::Branch { target } | Op::TailBranch { target } => {
                    if entry <= target && target < end {
                        pending.push_back(target);
                    } else if let Some(e) = targets.get(&target) {
                        sites.insert(
                            pc,
                            TerminalSite {
                                pc,
                                target: e.source(),
                                kind: SiteKind::Tail,
                            },
                        );
                        used.insert(target, (*e).clone());
                    } else {
                        returns.insert(pc);
                    }
                }
                Op::Call { target } => {
                    if let Some(e) = targets.get(&target) {
                        sites.insert(
                            pc,
                            TerminalSite {
                                pc,
                                target: e.source(),
                                kind: SiteKind::Call,
                            },
                        );
                        used.insert(target, (*e).clone());
                    } else {
                        pending.push_back(pc + 4);
                    }
                }
                Op::CompareBranch { target, .. }
                | Op::BranchCond { target, .. }
                | Op::TestBit { target, .. } => {
                    pending.push_back(target);
                    pending.push_back(pc + 4);
                }
                _ => pending.push_back(pc + 4),
            }
            instructions.insert(pc, ins);
        }
        let dependencies = used.into_values().collect::<Vec<_>>();
        let requirements = merge(dependencies.iter().flat_map(|e| e.requirements()))?;
        Ok(Analysis {
            image: self.clone(),
            source,
            instructions: instructions.into_values().collect(),
            issues: issues.into_values().collect(),
            returns: returns.into_iter().collect(),
            sites: sites.into_values().collect(),
            dependencies,
            requirements,
        })
    }
}
