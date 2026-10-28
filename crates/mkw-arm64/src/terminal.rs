//! Immutable, image-scoped normal-return control evidence. This is analysis only:
//! no existing lifter, CFG admission, emitter, runtime or original-execution gate
//! consumes these facts. There is no name/ABI whitelist or import/GOT assumption.
pub mod import;
use crate::{decode, Instruction, Op};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    sync::Arc,
};
const MAX_BODY: usize = 1024 * 1024;
struct ImageData {
    base: u64,
    bytes: Box<[u8]>,
    digest: [u8; 32],
}
/// Clones share one immutable source namespace. Independently created images,
/// even with identical bytes/digests, cannot exchange facts implicitly.
#[derive(Clone)]
pub struct Image {
    data: Arc<ImageData>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct SourceStamp {
    pub image_digest: [u8; 32],
    pub text_base: u64,
    pub entry: u64,
    pub bytes: usize,
    pub body_digest: [u8; 32],
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SetupError {
    EmptyImage,
    Unaligned,
    Overflow,
    InvalidSpan,
    BodyTooLarge,
    ForeignImageFact,
    DuplicateFact,
    RecursiveFact,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum Issue {
    MissingEdge { pc: u64 },
    Decode { pc: u64, word: u32, message: String },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum SiteKind {
    Call,
    Tail,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct TerminalSite {
    pub pc: u64,
    pub target: SourceStamp,
    pub kind: SiteKind,
}
struct Evidence {
    image: Arc<ImageData>,
    source: SourceStamp,
    dependencies: Vec<NoReturn>,
    digest: [u8; 32],
}
/// Opaque evidence of no normal return in the checked control model. It is not
/// serializable/deserializable or constructible from a caller's boolean/name.
/// Owned source bytes and previously established dependencies remain alive.
#[derive(Clone)]
pub struct NoReturn {
    evidence: Arc<Evidence>,
}
impl NoReturn {
    pub fn source(&self) -> SourceStamp {
        self.evidence.source
    }
    /// Content identity of this derivation, not loaded-code authentication.
    pub fn proof_digest(&self) -> [u8; 32] {
        self.evidence.digest
    }
    pub fn supporting_facts(&self) -> &[NoReturn] {
        &self.evidence.dependencies
    }

    pub fn dependencies(&self) -> impl Iterator<Item = SourceStamp> + '_ {
        self.evidence.dependencies.iter().map(Self::source)
    }
}
pub struct Analysis {
    image: Arc<ImageData>,
    source: SourceStamp,
    instructions: Vec<Instruction>,
    issues: Vec<Issue>,
    returns: Vec<u64>,
    sites: Vec<TerminalSite>,
    dependencies: Vec<NoReturn>,
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
    /// Conditional graph closure under the supplied local facts, not admission
    /// by any existing compiler profile or authorization for native invocation.
    pub fn is_closed(&self) -> bool {
        self.issues.is_empty()
    }
    pub fn no_return_fact(&self) -> Option<NoReturn> {
        if !self.is_closed() || !self.returns.is_empty() {
            return None;
        }
        let mut hash = Sha256::new();
        hash.update(b"mkw-local-normal-return-proof-v1\0");
        hash.update(self.source.image_digest);
        hash.update(self.source.text_base.to_le_bytes());
        hash.update(self.source.entry.to_le_bytes());
        hash.update((self.source.bytes as u64).to_le_bytes());
        hash.update(self.source.body_digest);
        for fact in &self.dependencies {
            hash.update(fact.proof_digest());
        }
        let digest = hash.finalize().into();
        Some(NoReturn {
            evidence: Arc::new(Evidence {
                image: self.image.clone(),
                source: self.source,
                dependencies: self.dependencies.clone(),
                digest,
            }),
        })
    }
}
impl Image {
    pub fn new(base: u64, bytes: Vec<u8>) -> Result<Self, SetupError> {
        if bytes.is_empty() {
            return Err(SetupError::EmptyImage);
        }
        if base % 4 != 0 || bytes.len() % 4 != 0 {
            return Err(SetupError::Unaligned);
        }
        base.checked_add(bytes.len() as u64)
            .ok_or(SetupError::Overflow)?;
        let digest = Sha256::digest(&bytes).into();
        Ok(Self {
            data: Arc::new(ImageData {
                base,
                bytes: bytes.into_boxed_slice(),
                digest,
            }),
        })
    }
    pub fn digest(&self) -> [u8; 32] {
        self.data.digest
    }
    /// All reachable conditional paths and failures are retained. Direct calls
    /// normally fall through; only an opaque fact for the exact local entry can
    /// remove that continuation. Indirect tails remain possible normal returns.
    /// Unknown decodes collect issues and continue diagnostically, never certify.
    pub fn analyze(
        &self,
        entry: u64,
        bytes: usize,
        facts: &[NoReturn],
    ) -> Result<Analysis, SetupError> {
        if entry % 4 != 0 || bytes % 4 != 0 {
            return Err(SetupError::Unaligned);
        }
        if bytes == 0 {
            return Err(SetupError::InvalidSpan);
        }
        if bytes > MAX_BODY {
            return Err(SetupError::BodyTooLarge);
        }
        let end = entry
            .checked_add(bytes as u64)
            .ok_or(SetupError::Overflow)?;
        if entry < self.data.base || end > self.data.base + self.data.bytes.len() as u64 {
            return Err(SetupError::InvalidSpan);
        }
        let mut targets = BTreeMap::new();
        for fact in facts {
            if !Arc::ptr_eq(&self.data, &fact.evidence.image) {
                return Err(SetupError::ForeignImageFact);
            }
            let at = fact.source().entry;
            if at == entry {
                return Err(SetupError::RecursiveFact);
            }
            if targets.insert(at, fact).is_some() {
                return Err(SetupError::DuplicateFact);
            }
        }
        let offset = (entry - self.data.base) as usize;
        let source = SourceStamp {
            image_digest: self.data.digest,
            text_base: self.data.base,
            entry,
            bytes,
            body_digest: Sha256::digest(&self.data.bytes[offset..offset + bytes]).into(),
        };
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
                    } else if let Some(fact) = targets.get(&target) {
                        sites.insert(
                            pc,
                            TerminalSite {
                                pc,
                                target: fact.source(),
                                kind: SiteKind::Tail,
                            },
                        );
                        used.insert(target, (*fact).clone());
                    } else {
                        returns.insert(pc);
                    }
                }
                Op::Call { target } => {
                    if let Some(fact) = targets.get(&target) {
                        sites.insert(
                            pc,
                            TerminalSite {
                                pc,
                                target: fact.source(),
                                kind: SiteKind::Call,
                            },
                        );
                        used.insert(target, (*fact).clone());
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
        Ok(Analysis {
            image: self.data.clone(),
            source,
            instructions: instructions.into_values().collect(),
            issues: issues.into_values().collect(),
            returns: returns.into_iter().collect(),
            sites: sites.into_values().collect(),
            dependencies: used.into_values().collect(),
        })
    }
}
