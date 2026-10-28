//! Serialized diagnostic reservations over owned, checked normal memory.
//! This is a backend primitive, not a verified target profile or CPU ABI binding.
use crate::{GuestAddr, Memory};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Width {
    Byte,
    Half,
    Word,
    Double,
    Pair,
}
impl Width {
    pub fn bytes(self) -> usize {
        match self {
            Self::Byte => 1,
            Self::Half => 2,
            Self::Word => 4,
            Self::Double => 8,
            Self::Pair => 16,
        }
    }
}
impl TryFrom<usize> for Width {
    type Error = Error;
    fn try_from(n: usize) -> Result<Self, Error> {
        match n {
            1 => Ok(Self::Byte),
            2 => Ok(Self::Half),
            4 => Ok(Self::Word),
            8 => Ok(Self::Double),
            16 => Ok(Self::Pair),
            _ => Err(Error::Width),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Configuration,
    Width,
    Alignment,
    Owner,
    OwnerLimit,
    IdentityExhausted,
    Address,
    Memory(String),
}
#[derive(Clone, Copy, Debug)]
pub struct Config {
    pub granule: u64,
    pub max_owners: usize,
}
/// Explicit fixed address/capacity; no target address-space defaults are inferred.
#[derive(Clone, Copy, Debug)]
pub struct HeapSpec {
    pub base: GuestAddr,
    pub capacity: usize,
}
/// Capability for one owned heap reservation in one memory generation.
#[derive(Clone)]
pub struct Heap {
    authority: Arc<()>,
    spec: HeapSpec,
}
impl Heap {
    pub(crate) fn authority(&self) -> &Arc<()> {
        &self.authority
    }
    pub fn spec(&self) -> HeapSpec {
        self.spec
    }
}
/// Identity is private and scoped to one authority, including after release/recreation.
#[derive(Clone, Debug)]
pub struct Owner {
    authority: Arc<()>,
    id: u64,
}
impl Owner {
    pub(crate) fn same_identity(&self, other: &Self) -> bool {
        self.id == other.id && Arc::ptr_eq(&self.authority, &other.authority)
    }
}
#[derive(Clone, Copy, Debug)]
struct Reservation {
    address: GuestAddr,
    width: Width,
    granule: u64,
}
#[derive(Debug)]
pub struct ExclusiveMemory {
    memory: Memory,
    config: Config,
    authority: Arc<()>,
    owners: BTreeSet<u64>,
    reservations: BTreeMap<u64, Reservation>,
    next_id: u64,
}
impl ExclusiveMemory {
    /// Reserve zeroed backing storage before sealing. Initially no heap bytes
    /// are accessible. Ordinary mappings remain sealed; only this capability
    /// can resize the reserved prefix. Capacity remains bounded by MAX_IMAGE.
    pub fn with_heap(
        mut memory: Memory,
        config: Config,
        spec: HeapSpec,
    ) -> Result<(Self, Heap), Error> {
        if config.granule < 16 || !config.granule.is_power_of_two() || config.max_owners == 0 {
            return Err(Error::Configuration);
        }
        memory
            .reserve_heap(spec.base, spec.capacity)
            .map_err(Error::Memory)?;
        let owned = Self::new(memory, config)?;
        let heap = Heap {
            authority: owned.authority.clone(),
            spec,
        };
        Ok((owned, heap))
    }
    /// Grow/shrink only the explicit heap prefix under the coherent memory lock.
    /// Retained bytes survive; changed bytes are zeroed. Reservations whose
    /// granules overlap the changed span are invalidated, including ABA regrowth.
    /// No SVC result, process policy or virtual-address layout is inferred.
    pub fn resize_heap(&mut self, heap: &Heap, size: usize) -> Result<(), Error> {
        if !Arc::ptr_eq(&self.authority, &heap.authority) {
            return Err(Error::Owner);
        }
        let old = self
            .memory
            .resize_heap(heap.spec.base, heap.spec.capacity, size)
            .map_err(Error::Memory)?;
        if old != size {
            let first = heap.spec.base.0 + old.min(size) as u64;
            let last = heap.spec.base.0 + old.max(size) as u64 - 1;
            let mask = self.config.granule - 1;
            self.reservations
                .retain(|_, r| last < r.granule || first > (r.granule | mask));
        }
        Ok(())
    }
    pub fn heap_size(&self, heap: &Heap) -> Result<usize, Error> {
        if !Arc::ptr_eq(&self.authority, &heap.authority) {
            return Err(Error::Owner);
        }
        self.memory
            .heap_size(heap.spec.base, heap.spec.capacity)
            .map_err(Error::Memory)
    }
    pub(crate) fn stack_authority(&self, base: GuestAddr, size: usize) -> Option<Arc<()>> {
        self.memory
            .exact_rw_region(base, size)
            .then(|| self.authority.clone())
    }
    pub(crate) fn authority(&self) -> Arc<()> {
        self.authority.clone()
    }
    pub(crate) fn has_authority(&self, authority: &Arc<()>) -> bool {
        Arc::ptr_eq(&self.authority, authority)
    }
    pub fn new(mut memory: Memory, config: Config) -> Result<Self, Error> {
        if config.granule < 16 || !config.granule.is_power_of_two() || config.max_owners == 0 {
            return Err(Error::Configuration);
        }
        memory.seal();
        Ok(Self {
            memory,
            config,
            authority: Arc::new(()),
            owners: BTreeSet::new(),
            reservations: BTreeMap::new(),
            next_id: 1,
        })
    }
    pub fn new_owner(&mut self) -> Result<Owner, Error> {
        if self.owners.len() >= self.config.max_owners {
            return Err(Error::OwnerLimit);
        }
        let next = self
            .next_id
            .checked_add(1)
            .ok_or(Error::IdentityExhausted)?;
        let id = self.next_id;
        self.next_id = next;
        self.owners.insert(id);
        Ok(Owner {
            authority: self.authority.clone(),
            id,
        })
    }
    fn owner(&self, owner: &Owner) -> Result<u64, Error> {
        if !Arc::ptr_eq(&owner.authority, &self.authority) || !self.owners.contains(&owner.id) {
            return Err(Error::Owner);
        }
        Ok(owner.id)
    }
    pub fn clear(&mut self, owner: &Owner) -> Result<(), Error> {
        let id = self.owner(owner)?;
        self.reservations.remove(&id);
        Ok(())
    }
    pub fn release(&mut self, owner: &Owner) -> Result<(), Error> {
        let id = self.owner(owner)?;
        self.reservations.remove(&id);
        self.owners.remove(&id);
        Ok(())
    }
    fn aligned(address: GuestAddr, width: Width) -> Result<(), Error> {
        if address.0 % (width.bytes() as u64) != 0 {
            Err(Error::Alignment)
        } else {
            Ok(())
        }
    }
    pub(crate) fn validate_owner(&self, owner: &Owner) -> Result<(), Error> {
        self.owner(owner).map(|_| ())
    }
    pub(crate) fn read_into(&self, address: GuestAddr, bytes: &mut [u8]) -> Result<(), Error> {
        self.memory.read_into(address, bytes).map_err(Error::Memory)
    }
    pub(crate) fn validate_access(
        &self,
        address: GuestAddr,
        bytes: usize,
        write: bool,
    ) -> Result<(), Error> {
        self.memory
            .spans(address, bytes, write)
            .map(|_| ())
            .map_err(Error::Memory)
    }
    pub fn read(&self, address: GuestAddr, width: Width) -> Result<u128, Error> {
        let mut bytes = [0; 16];
        self.memory
            .read_into(address, &mut bytes[..width.bytes()])
            .map_err(Error::Memory)?;
        Ok(u128::from_le_bytes(bytes))
    }
    /// A failed load leaves prior reservations intact. Trap effects belong to the future binding.
    pub fn load(&mut self, owner: &Owner, address: GuestAddr, width: Width) -> Result<u128, Error> {
        let id = self.owner(owner)?;
        Self::aligned(address, width)?;
        let value = self.read(address, width)?;
        self.reservations.insert(
            id,
            Reservation {
                address,
                width,
                granule: address.0 & !(self.config.granule - 1),
            },
        );
        Ok(value)
    }
    /// Every registered-owner attempt consumes its reservation, including failed/faulting stores.
    pub fn store_conditional(
        &mut self,
        owner: &Owner,
        address: GuestAddr,
        width: Width,
        value: u128,
    ) -> Result<bool, Error> {
        let id = self.owner(owner)?;
        let reservation = self.reservations.remove(&id);
        Self::aligned(address, width)?;
        if !reservation.is_some_and(|r| r.address == address && r.width == width) {
            return Ok(false);
        }
        self.write(address, &value.to_le_bytes()[..width.bytes()])?;
        Ok(true)
    }
    /// Commit a host service's discontiguous outputs under the same authority.
    /// Mappings cannot change inside this owned borrow; every range validates
    /// before the first write or reservation invalidation.
    pub(crate) fn write_batch(&mut self, writes: &[(GuestAddr, &[u8])]) -> Result<(), Error> {
        for (address, bytes) in writes {
            self.validate_access(*address, bytes.len(), true)?;
        }
        for (address, bytes) in writes {
            self.write(*address, bytes)
                .expect("validated unchanged owned memory mappings");
        }
        Ok(())
    }
    /// All transfers validate completely before mutation; failed ordinary stores do not invalidate.
    pub fn write(&mut self, address: GuestAddr, bytes: &[u8]) -> Result<(), Error> {
        let last = if bytes.is_empty() {
            None
        } else {
            Some(
                address
                    .0
                    .checked_add(u64::try_from(bytes.len() - 1).map_err(|_| Error::Address)?)
                    .ok_or(Error::Address)?,
            )
        };
        self.memory
            .write_bytes(address, bytes)
            .map_err(Error::Memory)?;
        if let Some(last) = last {
            let mask = self.config.granule - 1;
            self.reservations.retain(|_, r| {
                let end = r.granule | mask;
                last < r.granule || address.0 > end
            });
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::Permissions;
    fn memory(granule: u64, owners: usize) -> ExclusiveMemory {
        let mut m = Memory::default();
        m.map(GuestAddr(0x1000), vec![0; 64], Permissions::RW, "rw")
            .unwrap();
        m.map(GuestAddr(0x2000), vec![0; 16], Permissions::R, "ro")
            .unwrap();
        ExclusiveMemory::new(
            m,
            Config {
                granule,
                max_owners: owners,
            },
        )
        .unwrap()
    }
    #[test]
    fn every_width_success_clear_width_and_address_mismatch() {
        for width in [
            Width::Byte,
            Width::Half,
            Width::Word,
            Width::Double,
            Width::Pair,
        ] {
            let mut m = memory(16, 1);
            let owner = m.new_owner().unwrap();
            let value = u128::MAX - 0x12345678;
            let mask = if width == Width::Pair {
                u128::MAX
            } else {
                (1u128 << (width.bytes() * 8)) - 1
            };
            m.write(GuestAddr(0x1000), &value.to_le_bytes()[..width.bytes()])
                .unwrap();
            assert_eq!(m.load(&owner, GuestAddr(0x1000), width), Ok(value & mask));
            assert_eq!(
                m.store_conditional(&owner, GuestAddr(0x1000), width, 42),
                Ok(true)
            );
            assert_eq!(m.read(GuestAddr(0x1000), width), Ok(42 & mask));
            assert_eq!(
                m.store_conditional(&owner, GuestAddr(0x1000), width, 77),
                Ok(false)
            );
            m.load(&owner, GuestAddr(0x1000), width).unwrap();
            m.clear(&owner).unwrap();
            assert_eq!(
                m.store_conditional(&owner, GuestAddr(0x1000), width, 77),
                Ok(false)
            );
            m.load(&owner, GuestAddr(0x1000), width).unwrap();
            assert_eq!(
                m.store_conditional(&owner, GuestAddr(0x1010), width, 77),
                Ok(false)
            );
            assert_eq!(
                m.store_conditional(&owner, GuestAddr(0x1000), width, 77),
                Ok(false)
            );
            m.load(&owner, GuestAddr(0x1000), width).unwrap();
            let different = if width == Width::Byte {
                Width::Half
            } else {
                Width::Byte
            };
            assert_eq!(
                m.store_conditional(&owner, GuestAddr(0x1000), different, 77),
                Ok(false)
            );
        }
    }
    #[test]
    fn granule_writes_invalidate_all_owners_even_unchanged_bytes() {
        for granule in [16, 32, 64] {
            for offset in 0..64 {
                for chosen in 0..3 {
                    let mut m = memory(granule, 3);
                    let owners = [
                        m.new_owner().unwrap(),
                        m.new_owner().unwrap(),
                        m.new_owner().unwrap(),
                    ];
                    let addresses = [0x1000, 0x1008, 0x1020];
                    let widths = [Width::Word, Width::Double, Width::Word];
                    for i in 0..3 {
                        m.load(&owners[i], GuestAddr(addresses[i]), widths[i])
                            .unwrap();
                    }
                    m.write(GuestAddr(0x1000 + offset), &[0]).unwrap();
                    for i in 0..3 {
                        let survives = (addresses[i] - 0x1000) / granule != offset / granule;
                        let id = m.owner(&owners[i]).unwrap();
                        assert_eq!(m.reservations.contains_key(&id), survives);
                    }
                    let survives = (addresses[chosen] - 0x1000) / granule != offset / granule;
                    assert_eq!(
                        m.store_conditional(
                            &owners[chosen],
                            GuestAddr(addresses[chosen]),
                            widths[chosen],
                            0
                        ),
                        Ok(survives)
                    );
                    assert_eq!(
                        m.store_conditional(
                            &owners[chosen],
                            GuestAddr(addresses[chosen]),
                            widths[chosen],
                            0
                        ),
                        Ok(false)
                    );
                }
            }
        }
    }
    #[test]
    fn successful_owner_store_invalidates_peers_and_empty_write_keeps_reservation() {
        let mut m = memory(16, 3);
        let a = m.new_owner().unwrap();
        let b = m.new_owner().unwrap();
        let c = m.new_owner().unwrap();
        for (owner, address) in [(&a, 0x1000), (&b, 0x1008), (&c, 0x1020)] {
            m.load(owner, GuestAddr(address), Width::Word).unwrap();
        }
        m.write(GuestAddr(0x1000), &[]).unwrap();
        assert_eq!(
            m.store_conditional(&a, GuestAddr(0x1000), Width::Word, 9),
            Ok(true)
        );
        assert_eq!(
            m.store_conditional(&b, GuestAddr(0x1008), Width::Word, 7),
            Ok(false)
        );
        assert_eq!(
            m.store_conditional(&c, GuestAddr(0x1020), Width::Word, 5),
            Ok(true)
        );
        assert_eq!(m.read(GuestAddr(0x1000), Width::Word), Ok(9));
        assert_eq!(m.read(GuestAddr(0x1008), Width::Word), Ok(0));
        assert_eq!(m.read(GuestAddr(0x1020), Width::Word), Ok(5));
    }
    #[test]
    fn ownership_is_per_authority_quota_release_and_no_identity_wrap() {
        let mut a = memory(16, 1);
        let mut b = memory(16, 1);
        let x = a.new_owner().unwrap();
        let y = b.new_owner().unwrap();
        assert!(matches!(a.new_owner(), Err(Error::OwnerLimit)));
        assert_eq!(
            a.load(&y, GuestAddr(0x1000), Width::Word),
            Err(Error::Owner)
        );
        a.load(&x, GuestAddr(0x1000), Width::Word).unwrap();
        let alias = x.clone();
        a.release(&x).unwrap();
        assert_eq!(
            a.store_conditional(&alias, GuestAddr(0x1000), Width::Word, 1),
            Err(Error::Owner)
        );
        let next = a.new_owner().unwrap();
        assert_ne!(next.id, x.id);
        a.release(&next).unwrap();
        a.next_id = u64::MAX;
        assert!(matches!(a.new_owner(), Err(Error::IdentityExhausted)));
    }
    #[test]
    fn alignment_permission_faults_and_overflow_do_not_write() {
        let mut m = memory(16, 2);
        let owner = m.new_owner().unwrap();
        m.load(&owner, GuestAddr(0x1000), Width::Word).unwrap();
        assert_eq!(
            m.load(&owner, GuestAddr(0x1001), Width::Word),
            Err(Error::Alignment)
        );
        assert!(m.load(&owner, GuestAddr(0x3000), Width::Word).is_err());
        assert!(m.write(GuestAddr(0x103f), &[1, 2]).is_err());
        assert_eq!(m.write(GuestAddr(u64::MAX), &[1, 2]), Err(Error::Address));
        assert!(m
            .store_conditional(&owner, GuestAddr(0x1000), Width::Word, 5)
            .unwrap());
        m.load(&owner, GuestAddr(0x2000), Width::Word).unwrap();
        assert!(m
            .store_conditional(&owner, GuestAddr(0x2000), Width::Word, 9)
            .is_err());
        assert_eq!(m.read(GuestAddr(0x2000), Width::Word), Ok(0));
        assert_eq!(
            m.store_conditional(&owner, GuestAddr(0x2000), Width::Word, 9),
            Ok(false)
        );
    }
    #[test]
    fn cross_region_write_is_atomic_and_invalidates_every_overlapping_granule() {
        let mut mem = Memory::default();
        mem.map(GuestAddr(0x1000), vec![0; 16], Permissions::RW, "a")
            .unwrap();
        mem.map(GuestAddr(0x1010), vec![0; 16], Permissions::RW, "b")
            .unwrap();
        mem.map(GuestAddr(0x1020), vec![0; 16], Permissions::R, "c")
            .unwrap();
        let mut m = ExclusiveMemory::new(
            mem,
            Config {
                granule: 16,
                max_owners: 2,
            },
        )
        .unwrap();
        let a = m.new_owner().unwrap();
        let b = m.new_owner().unwrap();
        m.load(&a, GuestAddr(0x1000), Width::Pair).unwrap();
        m.load(&b, GuestAddr(0x1010), Width::Pair).unwrap();
        assert!(m.write(GuestAddr(0x101f), &[1, 2]).is_err());
        assert_eq!(m.read(GuestAddr(0x1010), Width::Pair), Ok(0));
        m.write(GuestAddr(0x100f), &[1, 2]).unwrap();
        assert_eq!(
            m.store_conditional(&a, GuestAddr(0x1000), Width::Pair, 7),
            Ok(false)
        );
        assert_eq!(
            m.store_conditional(&b, GuestAddr(0x1010), Width::Pair, 7),
            Ok(false)
        );
    }
    #[test]
    fn invalid_configuration_and_widths_refuse() {
        for granule in [0, 1, 8, 24] {
            assert!(ExclusiveMemory::new(
                Memory::default(),
                Config {
                    granule,
                    max_owners: 1
                }
            )
            .is_err());
        }
        assert!(ExclusiveMemory::new(
            Memory::default(),
            Config {
                granule: 16,
                max_owners: 0
            }
        )
        .is_err());
        for n in [0, 3, 6, 12, 24, 32] {
            assert_eq!(Width::try_from(n), Err(Error::Width));
        }
    }
}
