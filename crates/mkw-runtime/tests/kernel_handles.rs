use mkw_runtime::{
    kernel_handles::{Error, Handle, Table},
    owned_events::{Factory, ReadEnd, WriteEnd},
};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
struct Count(Arc<AtomicUsize>);
impl Drop for Count {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}
#[test]
fn copied_pins_outlive_source_handles_and_are_typed_namespaced_and_quota_bounded() {
    let a = Table::new(2).unwrap();
    let b = Table::new(1).unwrap();
    let drops = Arc::new(AtomicUsize::new(0));
    let h = a.insert(Arc::new(Count(drops.clone()))).unwrap();
    let legacy = mkw_runtime::events::Events::new(1).unwrap();
    let old = legacy.create().unwrap();
    assert!(h.raw() < 0x40000000 && h.raw() != old.write.raw() && h.raw() != old.read.raw());
    assert_eq!(
        a.get::<Count>(Handle::from_raw(old.read.raw()))
            .err()
            .map(|_| ()),
        Some(())
    );
    assert_eq!(
        legacy.signal(mkw_runtime::events::Handle::from_raw(h.raw())),
        Err(mkw_runtime::events::Error::InvalidHandle)
    );
    legacy.close(old.write).unwrap();
    legacy.close(old.read).unwrap();

    assert_eq!(b.get::<Count>(h).err(), Some(Error::InvalidHandle));
    assert_eq!(a.get::<u64>(h).err(), Some(Error::WrongType));
    let pin = a.export(h).unwrap();
    a.close(h).unwrap();
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    let copied = b.import(pin.clone()).unwrap();
    assert_ne!(copied, h);
    assert_eq!(b.import(pin.clone()).err(), Some(Error::Quota));
    assert_eq!(a.close(h), Err(Error::InvalidHandle));
    b.close(copied).unwrap();
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    drop(pin);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
    let valid = a.insert(Arc::new(Count(drops.clone()))).unwrap();
    let wrong = a.insert(Arc::new(9u64)).unwrap();
    assert_eq!(
        a.get_many::<Count>(&[valid, wrong]).err(),
        Some(Error::WrongType)
    );
    assert_eq!(
        a.get_many::<Count>(&[valid, Handle::from_raw(0)]).err(),
        Some(Error::InvalidHandle)
    );
    let duplicates = a.get_many::<Count>(&[valid, valid]).unwrap();
    assert_eq!(duplicates.len(), 2);
    a.close(valid).unwrap();
    assert_eq!(drops.load(Ordering::SeqCst), 1);
    drop(duplicates);
    assert_eq!(drops.load(Ordering::SeqCst), 2);
    a.close(wrong).unwrap();

    assert_eq!(
        Table::new(0).unwrap().insert(Arc::new(3u64)).err(),
        Some(Error::Quota)
    );
    assert_eq!(Table::new(1024).err().map(|_| ()), Some(()));
    assert_eq!(
        a.get_many::<Count>(&[Handle::from_raw(0); 65])
            .err()
            .map(|_| ()),
        Some(())
    );
}
#[test]
fn real_transferred_event_endpoints_share_signal_state_and_final_reference_lifetime() {
    let f = Factory::new(1).unwrap();
    let service = Table::new(2).unwrap();
    let guest = Table::new(2).unwrap();
    let (w, r) = f.create(&service).unwrap();
    let writer = service.get::<WriteEnd>(w).unwrap();
    let exported = service.export(r).unwrap();
    let received = guest.import(exported).unwrap();
    service.close(r).unwrap();
    assert_eq!(service.get::<ReadEnd>(r).err().map(|_| ()), Some(()));
    assert!(guest.get::<WriteEnd>(received).is_err());
    let read = guest.get::<ReadEnd>(received).unwrap();
    assert!(!read.ready().unwrap());
    writer.signal().unwrap();
    assert!(read.ready().unwrap());
    assert!(read.ready().unwrap());
    read.reset().unwrap();
    assert!(!read.ready().unwrap());
    writer.signal().unwrap();
    service.close(w).unwrap();
    drop(writer);
    assert!(read.ready().unwrap());
    assert!(f.create(&service).is_err());
    guest.close(received).unwrap();
    assert!(f.create(&service).is_err());
    drop(read);
    let (x, y) = f.create(&service).unwrap();
    service.close(x).unwrap();
    service.close(y).unwrap();
    let small = Table::new(1).unwrap();
    assert!(f.create(&small).is_err());
    let (x, y) = f.create(&service).unwrap();
    service.close(x).unwrap();
    service.close(y).unwrap();
}
#[test]
fn concurrent_exports_imports_and_drops_retain_unique_handles_without_locking_destructors() {
    let drops = Arc::new(AtomicUsize::new(0));
    let table = Arc::new(Table::new(16).unwrap());
    let mut threads = vec![];
    for _ in 0..8 {
        let table = table.clone();
        let drops = drops.clone();
        threads.push(std::thread::spawn(move || {
            let mut ids = vec![];
            for _ in 0..200 {
                let h = table.insert(Arc::new(Count(drops.clone()))).unwrap();
                let p = table.export(h).unwrap();
                table.close(h).unwrap();
                let j = table.import(p).unwrap();
                ids.extend([h.raw(), j.raw()]);
                table.close(j).unwrap();
            }
            ids
        }));
    }
    let ids = threads
        .into_iter()
        .flat_map(|t| t.join().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(ids.len(), 3200);
    assert_eq!(drops.load(Ordering::SeqCst), 1600);
    struct CloseOther {
        table: Arc<Table>,
        handle: Handle,
    }
    impl Drop for CloseOther {
        fn drop(&mut self) {
            self.table.close(self.handle).unwrap();
        }
    }
    let peer = table.insert(Arc::new(1u64)).unwrap();
    let h = table
        .insert(Arc::new(CloseOther {
            table: table.clone(),
            handle: peer,
        }))
        .unwrap();
    table.close(h).unwrap();
    assert!(table.get::<u64>(peer).is_err());
}
#[test]
fn explicit_shutdown_breaks_table_cycles_drops_outside_lock_and_preserves_transferred_pins() {
    let table = Arc::new(Table::new(2).unwrap());
    let drops = Arc::new(AtomicUsize::new(0));
    let peer = table.insert(Arc::new(Count(drops.clone()))).unwrap();
    let pin = table.export(peer).unwrap();
    struct Reenter {
        table: Arc<Table>,
        peer: Handle,
        drops: Arc<AtomicUsize>,
    }
    impl Drop for Reenter {
        fn drop(&mut self) {
            assert_eq!(self.table.close(self.peer), Err(Error::InvalidHandle));
            assert_eq!(self.table.insert(Arc::new(1u64)).err(), Some(Error::Closed));
            self.drops.fetch_add(1, Ordering::SeqCst);
        }
    }
    let cyclic = table
        .insert(Arc::new(Reenter {
            table: table.clone(),
            peer,
            drops: drops.clone(),
        }))
        .unwrap();
    table.shutdown().unwrap();
    table.shutdown().unwrap();
    assert_eq!(Arc::strong_count(&table), 1);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
    assert_eq!(table.export(cyclic).err().map(|_| ()), Some(()));
    assert_eq!(
        table.insert_pair(Arc::new(1u64), Arc::new(2u64)).err(),
        Some(Error::Closed)
    );
    assert_eq!(table.import(pin.clone()).err(), Some(Error::Closed));
    drop(pin);
    assert_eq!(drops.load(Ordering::SeqCst), 2);
    let f = Factory::new(1).unwrap();
    let source = Table::new(2).unwrap();
    let dest = Table::new(1).unwrap();
    let (_, r) = f.create(&source).unwrap();
    dest.import(source.export(r).unwrap()).unwrap();
    source.shutdown().unwrap();
    let fresh = Table::new(2).unwrap();
    assert!(f.create(&fresh).is_err());
    dest.shutdown().unwrap();
    let (w, r) = f.create(&fresh).unwrap();
    fresh.close(w).unwrap();
    fresh.close(r).unwrap();
}
