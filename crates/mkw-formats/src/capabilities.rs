//! Diagnostic facts from the selected Horizon capability field conventions.
//! Inspection does not authenticate ACID, enforce retail process policy, validate
//! physical mappings, or authorize execution. Unsupported descriptor kinds refuse.
use serde::Serialize;
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ThreadConstraints {
    /// Numeric priorities: smaller values have higher priority.
    pub minimum_priority: u8,
    pub maximum_priority: u8,
    pub minimum_virtual_core: u8,
    pub maximum_virtual_core: u8,
}
#[derive(Debug, Default, PartialEq, Eq, Serialize)]
pub struct KernelCapabilityFacts {
    pub threads: Option<ThreadConstraints>,
    pub allowed_svc_numbers: BTreeSet<u16>,
    pub program_type: Option<u8>,
    pub intended_kernel_version: Option<(u16, u8)>,
    pub handle_table_entries: Option<u16>,
    pub debug_flags: Option<u8>,
}
#[derive(Debug, PartialEq, Eq)]
pub enum CapabilityError {
    UnsupportedKind { index: usize, low_ones: u32 },
    Duplicate { index: usize },
    ReservedBits { index: usize },
    InvalidRange { index: usize },
}
impl KernelCapabilityFacts {
    /// Extract only the listed reference fields. No guessed default permissions,
    /// quotas, priorities or firmware values are supplied for missing records.
    pub fn inspect(words: &[u32]) -> Result<Self, CapabilityError> {
        let mut result = Self::default();
        let mut singletons = BTreeSet::new();
        let mut groups = BTreeSet::new();
        for (index, &word) in words.iter().enumerate() {
            let kind = word.trailing_ones();
            if kind == 32 {
                continue;
            }
            if matches!(kind, 3 | 13 | 14 | 15 | 16) && !singletons.insert(kind) {
                return Err(CapabilityError::Duplicate { index });
            }
            match kind {
                3 => {
                    let t = ThreadConstraints {
                        minimum_priority: ((word >> 10) & 63) as u8,
                        maximum_priority: ((word >> 4) & 63) as u8,
                        minimum_virtual_core: (word >> 16) as u8,
                        maximum_virtual_core: (word >> 24) as u8,
                    };
                    if t.minimum_priority < 4
                        || t.minimum_priority > t.maximum_priority
                        || t.minimum_virtual_core > t.maximum_virtual_core
                    {
                        return Err(CapabilityError::InvalidRange { index });
                    }
                    // Physical/virtual core availability requires target configuration.
                    result.threads = Some(t);
                }
                4 => {
                    let group = (word >> 29) as u16;
                    if !groups.insert(group) {
                        return Err(CapabilityError::Duplicate { index });
                    }
                    for bit in 0..24 {
                        if word & (1 << (5 + bit)) != 0 {
                            result.allowed_svc_numbers.insert(group * 24 + bit);
                        }
                    }
                }
                13 => {
                    if word >> 17 != 0 {
                        return Err(CapabilityError::ReservedBits { index });
                    }
                    result.program_type = Some(((word >> 14) & 7) as u8);
                }
                14 => {
                    let major = (word >> 19) as u16;
                    if major == 0 {
                        return Err(CapabilityError::InvalidRange { index });
                    }
                    result.intended_kernel_version = Some((major, ((word >> 15) & 15) as u8));
                }
                15 => {
                    if word >> 26 != 0 {
                        return Err(CapabilityError::ReservedBits { index });
                    }
                    result.handle_table_entries = Some(((word >> 16) & 1023) as u16);
                }
                16 => {
                    if word >> 20 != 0 {
                        return Err(CapabilityError::ReservedBits { index });
                    }
                    let flags = ((word >> 17) & 7) as u8;
                    if flags.count_ones() > 1 {
                        return Err(CapabilityError::InvalidRange { index });
                    }
                    result.debug_flags = Some(flags);
                }
                low_ones => return Err(CapabilityError::UnsupportedKind { index, low_ones }),
            }
        }
        Ok(result)
    }
}
