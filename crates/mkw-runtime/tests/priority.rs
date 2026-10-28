use mkw_runtime::{
    arbitration::{Actor, Arbitration, Error, Policy, Priorities, WAITERS},
    exclusive::{Config, ExclusiveMemory, Width},
    exclusive_native::Bridge,
    kernel_handles::Table,
    thread_queries::ThreadIdentity,
    GuestAddr, Memory, Permissions,
};
use std::sync::{Arc, Mutex};
fn fixture(n: usize, policy: Policy) -> (Arc<Mutex<ExclusiveMemory>>, Arc<Arbitration>, Table) {
    let mut raw = Memory::default();
    raw.map(
        GuestAddr(0x2000),
        vec![0; 4096],
        Permissions::RW,
        "mutex words",
    )
    .unwrap();
    let m = Arc::new(Mutex::new(
        ExclusiveMemory::new(
            raw,
            Config {
                granule: 16,
                max_owners: n + 1,
            },
        )
        .unwrap(),
    ));
    let d = Arbitration::with_policy(m.clone(), n, policy).unwrap();
    (m, d, Table::new(n).unwrap())
}
fn actor(m: &Arc<Mutex<ExclusiveMemory>>, d: &Arc<Arbitration>, t: &Table, p: u8) -> Actor {
    let h = ThreadIdentity::register(t, p).unwrap();
    d.bind(Bridge::new(m.clone()).unwrap(), t, h).unwrap()
}
fn tag(m: &Arc<Mutex<ExclusiveMemory>>, at: u64, a: &Actor) {
    m.lock()
        .unwrap()
        .write(GuestAddr(at), &(a.tag() | WAITERS).to_le_bytes())
        .unwrap();
}
fn read(m: &Arc<Mutex<ExclusiveMemory>>, at: u64) -> u32 {
    m.lock().unwrap().read(GuestAddr(at), Width::Word).unwrap() as u32
}
fn priority(a: &Actor, base: u8, effective: u8) {
    assert_eq!(a.priorities(), Ok(Priorities { base, effective }));
}
#[test]
fn donations_propagate_and_restore_across_multiple_owners_and_locks() {
    let (m, d, t) = fixture(4, Policy::InheritedPriority);
    let a = actor(&m, &d, &t, 50);
    let b = actor(&m, &d, &t, 40);
    let c = actor(&m, &d, &t, 10);
    let e = actor(&m, &d, &t, 20);
    tag(&m, 0x2000, &a);
    tag(&m, 0x2004, &b);
    tag(&m, 0x2008, &a);
    let wb = b.prepare_wait(&a, GuestAddr(0x2000)).unwrap().unwrap();
    priority(&a, 50, 40);
    let wc = c.prepare_wait(&b, GuestAddr(0x2004)).unwrap().unwrap();
    priority(&a, 50, 10);
    priority(&b, 40, 10);
    let we = e.prepare_wait(&a, GuestAddr(0x2008)).unwrap().unwrap();
    priority(&a, 50, 10);
    c.set_base_priority(30).unwrap();
    priority(&b, 40, 30);
    priority(&a, 50, 20);
    e.set_base_priority(5).unwrap();
    priority(&a, 50, 5);
    priority(&b, 40, 30);
    we.cancel();
    priority(&a, 50, 30);
    wc.cancel();
    priority(&a, 50, 40);
    priority(&b, 40, 40);
    b.set_base_priority(60).unwrap();
    priority(&a, 50, 50);
    priority(&b, 60, 60);
    wb.cancel();
    priority(&a, 50, 50);
    assert_eq!(d.pending(), Ok(0));
}
#[test]
fn changed_priority_reinserts_after_existing_equals_and_handoff_restores_donation() {
    let (m, d, t) = fixture(3, Policy::InheritedPriority);
    let a = actor(&m, &d, &t, 50);
    let b = actor(&m, &d, &t, 20);
    let c = actor(&m, &d, &t, 10);
    tag(&m, 0x2000, &a);
    let wb = b.prepare_wait(&a, GuestAddr(0x2000)).unwrap().unwrap();
    let wc = c.prepare_wait(&a, GuestAddr(0x2000)).unwrap().unwrap();
    b.set_base_priority(10).unwrap();
    priority(&a, 50, 10);
    a.unlock(GuestAddr(0x2000)).unwrap();
    assert_eq!(read(&m, 0x2000), c.tag() | WAITERS);
    wc.wait().unwrap();
    priority(&a, 50, 50);
    c.set_base_priority(40).unwrap();
    priority(&c, 40, 10);
    c.unlock(GuestAddr(0x2000)).unwrap();
    assert_eq!(read(&m, 0x2000), b.tag());
    wb.wait().unwrap();
    priority(&c, 40, 40);
    b.unlock(GuestAddr(0x2000)).unwrap();
    assert_eq!(read(&m, 0x2000), 0);
}
#[test]
fn fifo_stays_default_and_domain_base_changes_do_not_rewrite_thread_identity() {
    let (m, _, t) = fixture(3, Policy::Fifo);
    let d = Arbitration::new(m.clone(), 3).unwrap();
    let h = ThreadIdentity::register(&t, 50).unwrap();
    let pinned = t.get::<ThreadIdentity>(h).unwrap();
    let a = d.bind(Bridge::new(m.clone()).unwrap(), &t, h).unwrap();
    let b = actor(&m, &d, &t, 30);
    let c = actor(&m, &d, &t, 0);
    tag(&m, 0x2000, &a);
    let wb = b.prepare_wait(&a, GuestAddr(0x2000)).unwrap().unwrap();
    let wc = c.prepare_wait(&a, GuestAddr(0x2000)).unwrap().unwrap();
    priority(&a, 50, 50);
    a.set_base_priority(45).unwrap();
    priority(&a, 45, 45);
    assert_eq!(pinned.initial_priority(), 50);
    a.unlock(GuestAddr(0x2000)).unwrap();
    assert_eq!(read(&m, 0x2000), b.tag() | WAITERS);
    wb.wait().unwrap();
    b.unlock(GuestAddr(0x2000)).unwrap();
    wc.wait().unwrap();
}
#[test]
fn failures_cancellation_and_owner_loss_restore_only_live_donations() {
    let (m, d, t) = fixture(3, Policy::InheritedPriority);
    let a = actor(&m, &d, &t, 50);
    let b = actor(&m, &d, &t, 20);
    let c = actor(&m, &d, &t, 0);
    tag(&m, 0x2000, &a);
    tag(&m, 0x2004, &b);
    let wb = b.prepare_wait(&a, GuestAddr(0x2000)).unwrap().unwrap();
    let wc = c.prepare_wait(&b, GuestAddr(0x2004)).unwrap().unwrap();
    priority(&a, 50, 0);
    assert_eq!(c.set_base_priority(64), Err(Error::Configuration));
    priority(&c, 0, 0);
    priority(&a, 50, 0);
    assert_eq!(a.unlock(GuestAddr(0x3000)), Err(Error::Memory));
    priority(&a, 50, 0);
    let cancel = wc.cancellation();
    assert!(cancel.cancel().unwrap());
    priority(&b, 20, 20);
    priority(&a, 50, 20);
    assert_eq!(wc.wait(), Err(Error::Cancelled));
    drop(a);
    assert_eq!(wb.wait(), Err(Error::OwnerGone));
    priority(&b, 20, 20);
    let replacement = ExclusiveMemory::new(
        Memory::default(),
        Config {
            granule: 16,
            max_owners: 4,
        },
    )
    .unwrap();
    *m.lock().unwrap() = replacement;
    assert_eq!(b.priorities(), Err(Error::Stale));
    assert_eq!(b.set_base_priority(1), Err(Error::Stale));
}
// Independent oracle: a thread receives the minimum base priority of every
// descendant that reaches it. This does not mirror incremental restoration.
fn expected(base: &[u8; 4], parent: &[Option<usize>; 4]) -> [u8; 4] {
    let mut answer = *base;
    for (donor, &value) in base.iter().enumerate() {
        let mut at = Some(donor);
        while let Some(i) = at {
            answer[i] = answer[i].min(value);
            at = parent[i];
        }
    }
    answer
}
fn acyclic(parent: &[Option<usize>; 4]) -> bool {
    for start in 0..4 {
        let mut seen = [false; 4];
        let mut at = Some(start);
        while let Some(i) = at {
            if seen[i] {
                return false;
            }
            seen[i] = true;
            at = parent[i];
        }
    }
    true
}
#[test]
fn all_four_actor_forests_match_independent_donation_oracle_after_each_cancellation() {
    let mut checked = 0;
    for graph in 0..256 {
        let mut parent = [None; 4];
        let mut code = graph;
        for (i, p) in parent.iter_mut().enumerate() {
            let choices = (0..4)
                .filter(|&j| j != i)
                .map(Some)
                .chain([None])
                .collect::<Vec<_>>();
            *p = choices[code % 4];
            code /= 4;
        }
        if !acyclic(&parent) {
            continue;
        }
        for priorities in 0..81 {
            let mut base = [0; 4];
            let mut code = priorities;
            for v in &mut base {
                *v = [0, 10, 63][code % 3];
                code /= 3;
            }
            let (m, d, t) = fixture(4, Policy::InheritedPriority);
            let actors = base
                .iter()
                .map(|&p| actor(&m, &d, &t, p))
                .collect::<Vec<_>>();
            let mut waits = Vec::new();
            for i in 0..4 {
                waits.push(if let Some(owner) = parent[i] {
                    let at = 0x2000 + 4 * i as u64;
                    tag(&m, at, &actors[owner]);
                    actors[i]
                        .prepare_wait(&actors[owner], GuestAddr(at))
                        .unwrap()
                } else {
                    None
                });
            }
            let mut active = parent;
            for removed in 0..=4 {
                let oracle = expected(&base, &active);
                for i in 0..4 {
                    priority(&actors[i], base[i], oracle[i]);
                }
                if removed < 4 {
                    if let Some(w) = waits[removed].take() {
                        w.cancel();
                    }
                    active[removed] = None;
                }
            }
            checked += 1;
        }
    }
    assert_eq!(checked, 10125);
}
#[test]
fn long_dependency_chain_restores_without_recursive_stack() {
    const N: usize = 64;
    let (m, d, t) = fixture(N, Policy::InheritedPriority);
    let actors = (0..N)
        .map(|i| actor(&m, &d, &t, if i == N - 1 { 0 } else { 63 }))
        .collect::<Vec<_>>();
    let mut waits = Vec::new();
    for i in 1..N {
        let at = 0x2000 + 4 * i as u64;
        tag(&m, at, &actors[i - 1]);
        waits.push(
            actors[i]
                .prepare_wait(&actors[i - 1], GuestAddr(at))
                .unwrap()
                .unwrap(),
        );
    }
    for a in &actors {
        assert_eq!(a.priorities().unwrap().effective, 0);
    }
    waits.pop().unwrap().cancel();
    for a in &actors[..N - 1] {
        priority(a, 63, 63);
    }
    d.shutdown().unwrap();
    for w in waits {
        assert_eq!(w.wait(), Err(Error::Shutdown));
    }
}
