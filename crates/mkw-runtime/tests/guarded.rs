use mkw_arm64::terminal::{conditional::Effect, contract::Contract, Image};
use mkw_codegen::guarded::Artifact;
use mkw_runtime::{
    exclusive::{Config, ExclusiveMemory},
    exclusive_native::{Bridge, CpuV10},
    guarded::{Routed, Router, Target},
    services::Services,
    GuestAddr, Memory, Permissions,
};
use std::{
    path::Path,
    process::Command,
    sync::{Arc, Mutex},
};
fn bytes(w: &[u32]) -> Vec<u8> {
    w.iter().flat_map(|w| w.to_le_bytes()).collect()
}
#[test]
fn guarded_import_checks_actual_target_before_invocation_and_keeps_exact_budget() {
    for tail in [false, true] {
        let mut words = vec![0xd503201fu32; 40];
        words[0] = if tail { 0x14000010 } else { 0x94000010 };
        words[16..20].copy_from_slice(&[0xd0000010, 0xf9400e11, 0x91006210, 0xd61f0220]);
        words[32] = 0x14000000;
        words[36..38].copy_from_slice(&[0xd28000e0, 0xd65f03c0]);
        let image = Image::new(0, bytes(&words)).unwrap();
        let local = image
            .analyze(0x80, 4, &[])
            .unwrap()
            .no_return_fact()
            .unwrap();
        let loop_effect = Effect::from_local(&local);
        let alias = image
            .conditional_import(&image.import_thunk(0x40, 0).unwrap(), &loop_effect)
            .unwrap();
        if !tail {
            assert!(mkw_arm64::lift_function(&bytes(&words[..1]), 0, 0).is_err());
        }
        let contracts = vec![
            Contract::analyze(&image, 0, 4, std::slice::from_ref(&alias)).unwrap(),
            Contract::effect(&image, &alias).unwrap(),
            Contract::effect(&image, &loop_effect).unwrap(),
            Contract::plain_services(&image, 0x90, 8).unwrap(),
        ];
        let artifacts = contracts
            .into_iter()
            .enumerate()
            .map(|(i, c)| Artifact::new(c, &format!("mkw_gv4_entry_{i}")).unwrap())
            .collect::<Vec<_>>();
        let dir =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../private/testing/guarded-native");
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join(format!("synthetic-{tail}.c"));
        std::fs::write(
            &src,
            artifacts.iter().map(Artifact::code).collect::<String>(),
        )
        .unwrap();
        for opt in ["-O0", "-O2"] {
            let file = dir.join(format!("synthetic-{tail}{opt}.so"));
            let cc = Command::new("cc")
                .args([
                    "-std=c11",
                    opt,
                    "-Wall",
                    "-Wextra",
                    "-Werror",
                    "-shared",
                    "-fPIC",
                    "-fsanitize=undefined",
                    "-fno-sanitize-recover=undefined",
                    "-I",
                ])
                .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../native"))
                .arg(&src)
                .arg("-o")
                .arg(&file)
                .output()
                .unwrap();
            assert!(
                cc.status.success(),
                "{}",
                String::from_utf8_lossy(&cc.stderr)
            );
            let lib = unsafe { libloading::Library::new(file).unwrap() };
            let targets = artifacts
                .iter()
                .map(|a| unsafe {
                    Target::load(
                        &lib,
                        a.name().as_bytes(),
                        a.contract().clone(),
                        a.producer(),
                        0x400000,
                    )
                    .unwrap()
                })
                .collect();
            let router = Router::new(targets, 16).unwrap();
            let services = Services::new(vec![]).unwrap();
            let mut memory = Memory::default();
            memory
                .map(
                    GuestAddr(0x402000),
                    vec![0; 64],
                    Permissions::RW,
                    "mutable synthetic slot",
                )
                .unwrap();
            let owned = Arc::new(Mutex::new(
                ExclusiveMemory::new(
                    memory,
                    Config {
                        granule: 16,
                        max_owners: 1,
                    },
                )
                .unwrap(),
            ));
            let mut bridge = Bridge::new(owned.clone()).unwrap();
            for target in [
                0x400080u64,
                0x400090,
                0x400040,
                0x400044,
                0x400048,
                0x40004c,
                0x400080,
            ] {
                owned
                    .lock()
                    .unwrap()
                    .write(GuestAddr(0x402018), &target.to_le_bytes())
                    .unwrap();
                for initial in 0..=20u64 {
                    let mut cpu = CpuV10::default();
                    cpu.base.x[0] = 55;
                    cpu.base.x[16] = 66;
                    cpu.base.x[17] = 77;
                    cpu.base.x[30] = 0x500000;
                    cpu.base.module_base = 0xdead0000;
                    let mut budget = initial;
                    let status = {
                        let mut a = bridge.attach(&mut cpu).unwrap();
                        let mut r = Routed::attach(&mut a, &router, &services).unwrap();
                        assert_eq!(std::mem::size_of_val(r.table()), 112);
                        r.run(GuestAddr(0x400000), &mut budget).unwrap()
                    };
                    if target == 0x400080 || initial <= 5 {
                        assert_eq!(status, 2);
                        assert_eq!(budget, 0);
                    } else {
                        assert_eq!(status, 3);
                        assert_eq!(budget, initial - 5);
                        assert_eq!(cpu.base.fault_address, target);
                    }
                    let executed = initial.min(5);
                    let pc = match executed {
                        0 => 0x400000,
                        1 => 0x400040,
                        2 => 0x400044,
                        3 => 0x400048,
                        4 => 0x40004c,
                        _ => target,
                    };
                    assert_eq!(cpu.base.pc, pc);
                    assert_eq!(cpu.base.x[0], 55);
                    assert_eq!(
                        cpu.base.x[30],
                        if initial == 0 || tail {
                            0x500000
                        } else {
                            0x400004
                        }
                    );
                    assert_eq!(cpu.base.module_base, 0xdead0000);
                    assert_eq!(
                        cpu.base.x[16],
                        if initial < 2 {
                            66
                        } else if initial < 4 {
                            0x402000
                        } else {
                            0x402018
                        }
                    );
                    assert_eq!(cpu.base.x[17], if initial < 3 { 77 } else { target });
                }
            }
        }
    }
}

fn compile(artifacts: &[Artifact], label: &str, opt: &str, extra: &str) -> libloading::Library {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../private/testing/guarded-native")
        .join(label);
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("synthetic.c");
    std::fs::write(
        &src,
        artifacts.iter().map(Artifact::code).collect::<String>() + extra,
    )
    .unwrap();
    let file = dir.join(format!("synthetic{opt}.so"));
    let cc = Command::new("cc")
        .args([
            "-std=c11",
            opt,
            "-Wall",
            "-Wextra",
            "-Werror",
            "-shared",
            "-fPIC",
            "-fsanitize=undefined",
            "-fno-sanitize-recover=undefined",
            "-I",
        ])
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../native"))
        .arg(&src)
        .arg("-o")
        .arg(&file)
        .output()
        .unwrap();
    assert!(
        cc.status.success(),
        "{}",
        String::from_utf8_lossy(&cc.stderr)
    );
    unsafe { libloading::Library::new(file).unwrap() }
}
fn target<'a>(lib: &'a libloading::Library, a: &Artifact) -> Target<'a> {
    unsafe {
        Target::load(
            lib,
            a.name().as_bytes(),
            a.contract().clone(),
            a.producer(),
            0x400000,
        )
        .unwrap()
    }
}
fn memory() -> Arc<Mutex<ExclusiveMemory>> {
    let mut m = Memory::default();
    m.map(
        GuestAddr(0x402000),
        vec![0; 4096],
        Permissions::RW,
        "mutable synthetic slots",
    )
    .unwrap();
    Arc::new(Mutex::new(
        ExclusiveMemory::new(
            m,
            Config {
                granule: 16,
                max_owners: 1,
            },
        )
        .unwrap(),
    ))
}
#[test]
fn four_transitive_guards_preserve_returning_success_branch_and_reject_each_changed_slot() {
    let mut w = vec![0xd503201fu32; 256];
    w[..3].copy_from_slice(&[0xb4000040, 0x9400004b, 0xd65f03c0]);
    w[0x300 / 4] = 0x14000000;
    w[0x320 / 4..0x320 / 4 + 2].copy_from_slice(&[0xd28000e0, 0xd65f03c0]);
    for n in 0..4usize {
        let at = (0x100 + n * 16) / 4;
        let offset = 24 + n as u32 * 8;
        w[at..at + 4].copy_from_slice(&[
            0xd0000010,
            0xf9400211 | ((offset / 8) << 10),
            0x91000210 | (offset << 10),
            0xd61f0220,
        ]);
    }
    for n in 0..3usize {
        w[(0x200 + n * 16) / 4] = 0x97ffffc0;
    }
    let image = Image::new(0, bytes(&w)).unwrap();
    let mut e = Effect::from_local(
        &image
            .analyze(0x300, 4, &[])
            .unwrap()
            .no_return_fact()
            .unwrap(),
    );
    let mut contracts = vec![Contract::effect(&image, &e).unwrap()];
    for n in 0..4u64 {
        let a = image
            .conditional_import(&image.import_thunk(0x100 + n * 16, 0).unwrap(), &e)
            .unwrap();
        contracts.push(Contract::effect(&image, &a).unwrap());
        if n < 3 {
            e = image
                .analyze_conditionally(0x200 + n * 16, 4, &[a])
                .unwrap()
                .no_return_effect()
                .unwrap()
                .unwrap();
            contracts.push(Contract::effect(&image, &e).unwrap());
        } else {
            contracts.push(Contract::analyze(&image, 0, 12, std::slice::from_ref(&a)).unwrap());
        }
    }
    assert!(contracts.last().unwrap().claim().is_none());
    assert_eq!(
        contracts.last().unwrap().effects()[0].requirements().len(),
        4
    );
    contracts.push(Contract::plain_services(&image, 0x320, 8).unwrap());
    let artifacts = contracts
        .into_iter()
        .enumerate()
        .map(|(i, c)| Artifact::new(c, &format!("mkw_gv4_chain_{i}")).unwrap())
        .collect::<Vec<_>>();
    for opt in ["-O0", "-O2"] {
        let lib = compile(&artifacts, "four-chain", opt, "");
        let router = Router::new(artifacts.iter().map(|a| target(&lib, a)).collect(), 32).unwrap();
        let services = Services::new(vec![]).unwrap();
        let owned = memory();
        let mut bridge = Bridge::new(owned.clone()).unwrap();
        let providers = [0x400300u64, 0x400200, 0x400210, 0x400220];
        let initialize = || {
            for (n, p) in providers.iter().enumerate() {
                owned
                    .lock()
                    .unwrap()
                    .write(GuestAddr(0x402018 + n as u64 * 8), &p.to_le_bytes())
                    .unwrap();
            }
        };
        initialize();
        let states = [
            0, 4, 0x130, 0x134, 0x138, 0x13c, 0x220, 0x120, 0x124, 0x128, 0x12c, 0x210, 0x110,
            0x114, 0x118, 0x11c, 0x200, 0x100, 0x104, 0x108, 0x10c, 0x300,
        ];
        for initial in 0..=48u64 {
            let mut cpu = CpuV10::default();
            cpu.base.x[0] = 1;
            cpu.base.x[30] = 0x500000;
            let mut budget = initial;
            let status = {
                let mut a = bridge.attach(&mut cpu).unwrap();
                let mut r = Routed::attach(&mut a, &router, &services).unwrap();
                r.run(GuestAddr(0x400000), &mut budget).unwrap()
            };
            assert_eq!(
                (status, budget, cpu.base.pc, cpu.base.x[0]),
                (2, 0, 0x400000 + states[initial.min(21) as usize], 1)
            );
        }
        for n in 0..4u64 {
            initialize();
            owned
                .lock()
                .unwrap()
                .write(GuestAddr(0x402018 + n * 8), &0x400320u64.to_le_bytes())
                .unwrap();
            let mut cpu = CpuV10::default();
            cpu.base.x[0] = 1;
            cpu.base.x[30] = 0x500000;
            let mut budget = 100;
            let status = {
                let mut a = bridge.attach(&mut cpu).unwrap();
                let mut r = Routed::attach(&mut a, &router, &services).unwrap();
                r.run(GuestAddr(0x400000), &mut budget).unwrap()
            };
            assert_eq!(
                (status, budget, cpu.base.pc, cpu.base.x[0]),
                (3, 100 - (21 - 5 * n), 0x400320, 1)
            );
        }
        for initial in 0..=4u64 {
            let mut cpu = CpuV10::default();
            cpu.base.x[0] = 0;
            cpu.base.x[30] = 0x500000;
            let mut budget = initial;
            let status = {
                let mut a = bridge.attach(&mut cpu).unwrap();
                let mut r = Routed::attach(&mut a, &router, &services).unwrap();
                r.run(GuestAddr(0x400000), &mut budget).unwrap()
            };
            assert_eq!(status, if initial < 2 { 2 } else { 0 });
            assert_eq!(budget, initial.saturating_sub(2));
            assert_eq!(
                cpu.base.pc,
                match initial {
                    0 => 0x400000,
                    1 => 0x400008,
                    _ => 0x500000,
                }
            );
        }
    }
}
struct Mutate(std::sync::atomic::AtomicU64);
impl mkw_runtime::services::Service for Mutate {
    fn invoke(
        &self,
        _: &mkw_runtime::services::Call<'_>,
    ) -> Result<mkw_runtime::services::Reply, mkw_runtime::host_call::HostFault> {
        use mkw_runtime::services::{Outputs, Reply};
        Ok(Reply::Return(
            Outputs::default().write_memory(
                GuestAddr(0x402018),
                self.0
                    .load(std::sync::atomic::Ordering::SeqCst)
                    .to_le_bytes()
                    .to_vec(),
            )?,
        ))
    }
}
#[test]
fn nested_services_mutate_slot_after_preflight_and_restore_origin_after_refusal() {
    let mut w = vec![0xd503201fu32; 64];
    w[4..6].copy_from_slice(&[0x9400002c, 0x9400000b]);
    w[16..20].copy_from_slice(&[0xd0000010, 0xf9400e11, 0x91006210, 0xd61f0220]);
    w[32] = 0x14000000;
    w[36..38].copy_from_slice(&[0xd28000e0, 0xd65f03c0]);
    w[48..50].copy_from_slice(&[0xd400e001, 0xd65f03c0]);
    w[56..60].copy_from_slice(&[0xc85f7c41, 0xd400e001, 0xc8037c41, 0xd65f03c0]);
    let image = Image::new(0, bytes(&w)).unwrap();
    let loop_effect = Effect::from_local(
        &image
            .analyze(0x80, 4, &[])
            .unwrap()
            .no_return_fact()
            .unwrap(),
    );
    let alias = image
        .conditional_import(&image.import_thunk(0x40, 0).unwrap(), &loop_effect)
        .unwrap();
    let cs = vec![
        Contract::analyze(&image, 0x10, 8, std::slice::from_ref(&alias)).unwrap(),
        Contract::effect(&image, &alias).unwrap(),
        Contract::effect(&image, &loop_effect).unwrap(),
        Contract::plain_services(&image, 0x90, 8).unwrap(),
        Contract::plain_services(&image, 0xc0, 8).unwrap(),
        Contract::plain_services(&image, 0xe0, 16).unwrap(),
    ];
    let artifacts = cs
        .into_iter()
        .enumerate()
        .map(|(i, c)| Artifact::new(c, &format!("mkw_gv4_mutate_{i}")).unwrap())
        .collect::<Vec<_>>();
    for opt in ["-O0", "-O2"] {
        let lib = compile(&artifacts, "in-flight", opt, "");
        let router = Router::new(artifacts.iter().map(|a| target(&lib, a)).collect(), 16).unwrap();
        let mutator = Arc::new(Mutate(std::sync::atomic::AtomicU64::new(0x400090)));
        let services = Services::new(vec![(0x700, mutator.clone())]).unwrap();
        let owned = memory();
        owned
            .lock()
            .unwrap()
            .write(GuestAddr(0x402018), &0x400080u64.to_le_bytes())
            .unwrap();
        let mut bridge = Bridge::new(owned.clone()).unwrap();
        let mut cpu = CpuV10::default();
        let mut a = bridge.attach(&mut cpu).unwrap();
        let mut r = Routed::attach(&mut a, &router, &services).unwrap();
        r.cpu_mut().base.x[0] = 55;
        r.cpu_mut().base.x[30] = 0x500000;
        let mut budget = 30;
        assert_eq!(r.run(GuestAddr(0x400010), &mut budget).unwrap(), 3);
        assert_eq!(
            (budget, r.cpu_mut().base.x[0], r.cpu_mut().base.pc),
            (22, 55, 0x400090)
        );
        mutator
            .0
            .store(0x400080, std::sync::atomic::Ordering::SeqCst);
        r.cpu_mut().base.x[30] = 0x500000;
        budget = 30;
        assert_eq!(r.run(GuestAddr(0x400010), &mut budget).unwrap(), 2);
        assert_eq!((budget, r.cpu_mut().base.pc), (0, 0x400080));
        r.cpu_mut().base.x[30] = 0x500000;
        budget = 30;
        assert_eq!(r.run(GuestAddr(0x4000c0), &mut budget).unwrap(), 0);
        assert_eq!((budget, r.cpu_mut().base.pc), (28, 0x500000));
        owned
            .lock()
            .unwrap()
            .write(GuestAddr(0x402038), &5u64.to_le_bytes())
            .unwrap();
        r.cpu_mut().base.x[2] = 0x402038;
        r.cpu_mut().base.x[30] = 0x500000;
        budget = 10;
        assert_eq!(r.run(GuestAddr(0x4000e0), &mut budget).unwrap(), 0);
        assert_eq!(
            (budget, r.cpu_mut().base.x[1], r.cpu_mut().base.x[3]),
            (6, 5, 1)
        );
        assert_eq!(
            owned
                .lock()
                .unwrap()
                .read(GuestAddr(0x402038), mkw_runtime::exclusive::Width::Double)
                .unwrap(),
            5
        );
        assert_eq!(qemu_svc_monitor(), vec![5, 1, 5]);
    }
}
// Independent Linux/QEMU SVC monitor transport only, not a Horizon service oracle.
fn qemu_svc_monitor() -> Vec<u64> {
    fn imm(w: &mut Vec<u32>, r: u32, v: u64) {
        w.push(0xd2800000 | ((v as u32 & 65535) << 5) | r);
        for n in 1..4 {
            w.push(0xf2800000 | ((n as u32) << 21) | ((((v >> (16 * n)) as u32) & 65535) << 5) | r);
        }
    }
    let mut w = Vec::new();
    imm(&mut w, 2, 0x404000);
    imm(&mut w, 0, u64::MAX);
    imm(&mut w, 8, 57);
    w.extend([0xc85f7c41, 0xd4000001, 0xc8037c41]);
    imm(&mut w, 20, 0x404010);
    w.extend([0xf9000281, 0xf9000683, 0xf9400044, 0xf9000a84]);
    for (r, v) in [(0, 1), (1, 0x404010), (2, 24), (8, 64)] {
        imm(&mut w, r, v);
    }
    w.push(0xd4000001);
    imm(&mut w, 0, 0);
    imm(&mut w, 8, 93);
    w.push(0xd4000001);
    let mut elf = vec![0u8; 0x4028];
    elf[..7].copy_from_slice(b"\x7fELF\x02\x01\x01");
    for (at, v) in [(16, 2u16), (18, 183), (52, 64), (54, 56), (56, 1)] {
        elf[at..at + 2].copy_from_slice(&v.to_le_bytes());
    }
    for (at, v) in [(20, 1u32), (64, 1), (68, 7)] {
        elf[at..at + 4].copy_from_slice(&v.to_le_bytes());
    }
    let len = elf.len() as u64;
    for (at, v) in [
        (24, 0x401000u64),
        (32, 64),
        (80, 0x400000),
        (88, 0x400000),
        (96, len),
        (104, len),
        (112, 4096),
        (0x4000, 5),
    ] {
        elf[at..at + 8].copy_from_slice(&v.to_le_bytes());
    }
    let raw = bytes(&w);
    elf[0x1000..0x1000 + raw.len()].copy_from_slice(&raw);
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../private/testing/guarded-native/qemu-svc.elf");
    std::fs::write(&path, elf).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
    let result = Command::new("qemu-aarch64")
        .arg(path)
        .output()
        .expect("QEMU is required for guarded monitor transport test");
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(result.stdout.len(), 24);
    result
        .stdout
        .chunks_exact(8)
        .map(|b| u64::from_le_bytes(b.try_into().unwrap()))
        .collect()
}
#[test]
fn source_origin_dependency_layout_and_contradictory_return_refusals_are_explicit() {
    let mut w = vec![0xd503201fu32; 40];
    w[0] = 0x94000010;
    w[16..20].copy_from_slice(&[0xd0000010, 0xf9400e11, 0x91006210, 0xd61f0220]);
    w[32] = 0x14000000;
    let image = Image::new(0, bytes(&w)).unwrap();
    let seed = Effect::from_local(
        &image
            .analyze(0x80, 4, &[])
            .unwrap()
            .no_return_fact()
            .unwrap(),
    );
    let alias = image
        .conditional_import(&image.import_thunk(0x40, 0).unwrap(), &seed)
        .unwrap();
    let cs = [
        Contract::analyze(&image, 0, 4, std::slice::from_ref(&alias)).unwrap(),
        Contract::effect(&image, &alias).unwrap(),
        Contract::effect(&image, &seed).unwrap(),
        Contract::plain_services(&image, 0x80, 4).unwrap(),
    ];
    let artifacts = cs
        .into_iter()
        .enumerate()
        .map(|(i, c)| Artifact::new(c, &format!("mkw_gv4_checks_{i}")).unwrap())
        .collect::<Vec<_>>();
    assert!(mkw_codegen::emit_services_v3(artifacts[0].contract().instructions(), 0).is_err());
    let origin = artifacts[2]
        .origin()
        .iter()
        .map(u8::to_string)
        .collect::<Vec<_>>()
        .join(",");
    // Deliberately dishonest metadata/body fault injections, not trusted production
    // artifacts. Guards detect normal return and bad embedded preflight identity;
    // self-reported queries are not arbitrary machine-code authentication.
    let extra = format!(
        r#"
static const uint8_t injected_origin[32]={{{origin}}};
unsigned mkw_gv4_injected_return_abi_version(void){{return 10;}}
unsigned mkw_gv4_injected_return_exclusive_version(void){{return 4;}}
void mkw_gv4_injected_return_origin_v4(uint8_t*p){{for(unsigned i=0;i<32;++i)p[i]=injected_origin[i];}}
int mkw_gv4_injected_return(MkwCpuV10*f,uint64_t*b){{int s=mkw_preflight_v4(f,injected_origin,128);if(s)return s;if(!*b)return 2;--*b;f->base.x[0]=999;f->base.pc=f->base.x[30];return 0;}}
unsigned mkw_gv4_injected_origin_abi_version(void){{return 10;}}
unsigned mkw_gv4_injected_origin_exclusive_version(void){{return 4;}}
void mkw_gv4_injected_origin_origin_v4(uint8_t*p){{for(unsigned i=0;i<32;++i)p[i]=injected_origin[i];}}
int mkw_gv4_injected_origin(MkwCpuV10*f,uint64_t*b){{(void)b;const uint8_t wrong[32]={{0}};return mkw_preflight_v4(f,wrong,128);}}
unsigned mkw_gv4_wrongprofile_abi_version(void){{return 10;}}
unsigned mkw_gv4_wrongprofile_exclusive_version(void){{return 3;}}
size_t guarded_size(void){{return sizeof(MkwGuardedV4);}}
size_t guarded_prefix(void){{return offsetof(MkwGuardedV4,preflight);}}
size_t guarded_context(void){{return offsetof(MkwGuardedV4,guard_context);}}
"#
    );
    for opt in ["-O0", "-O2"] {
        let lib = compile(&artifacts, "contract-checks", opt, &extra);
        use mkw_runtime::guarded::OpsV4;
        for (name, expected) in [
            (b"guarded_size\0".as_slice(), std::mem::size_of::<OpsV4>()),
            (
                b"guarded_prefix\0".as_slice(),
                std::mem::offset_of!(OpsV4, preflight),
            ),
            (
                b"guarded_context\0".as_slice(),
                std::mem::offset_of!(OpsV4, guard_context),
            ),
        ] {
            let query = unsafe { lib.get::<unsafe extern "C" fn() -> usize>(name).unwrap() };
            assert_eq!(unsafe { query() }, expected);
        }
        let mut wrong = artifacts[2].producer();
        wrong[0] ^= 1;
        assert!(unsafe {
            Target::load(
                &lib,
                artifacts[2].name().as_bytes(),
                artifacts[2].contract().clone(),
                wrong,
                0x400000,
            )
        }
        .is_err());
        assert!(unsafe {
            Target::load(
                &lib,
                artifacts[0].name().as_bytes(),
                artifacts[2].contract().clone(),
                artifacts[2].producer(),
                0x400000,
            )
        }
        .is_err());
        assert!(unsafe {
            Target::load(
                &lib,
                b"mkw_gv4_wrongprofile",
                artifacts[2].contract().clone(),
                artifacts[2].producer(),
                0x400000,
            )
        }
        .is_err());
        let high = Image::new(0x1000, bytes(&[0x14000000])).unwrap();
        let high_effect = Effect::from_local(
            &high
                .analyze(0x1000, 4, &[])
                .unwrap()
                .no_return_fact()
                .unwrap(),
        );
        let high_contract = Contract::effect(&high, &high_effect).unwrap();
        assert!(unsafe {
            Target::load(
                &lib,
                b"missing_symbol",
                high_contract,
                artifacts[2].producer(),
                u64::MAX - 4095,
            )
        }
        .err()
        .unwrap()
        .contains("mapping overflow"));
        assert!(Router::new(
            vec![target(&lib, &artifacts[0]), target(&lib, &artifacts[1])],
            16
        )
        .is_err());
        assert!(Router::new(
            vec![
                target(&lib, &artifacts[0]),
                target(&lib, &artifacts[1]),
                target(&lib, &artifacts[3])
            ],
            16
        )
        .is_err());
        for (name, status, remaining, x0) in [
            (b"mkw_gv4_injected_return".as_slice(), 6, 14, 999),
            (b"mkw_gv4_injected_origin".as_slice(), 3, 15, 55),
        ] {
            let injected = unsafe {
                Target::load(
                    &lib,
                    name,
                    artifacts[2].contract().clone(),
                    artifacts[2].producer(),
                    0x400000,
                )
                .unwrap()
            };
            let router = Router::new(
                vec![
                    target(&lib, &artifacts[0]),
                    target(&lib, &artifacts[1]),
                    injected,
                ],
                16,
            )
            .unwrap();
            let services = Services::new(vec![]).unwrap();
            let owned = memory();
            owned
                .lock()
                .unwrap()
                .write(GuestAddr(0x402018), &0x400080u64.to_le_bytes())
                .unwrap();
            let mut bridge = Bridge::new(owned).unwrap();
            let mut cpu = CpuV10::default();
            cpu.base.x[0] = 55;
            let mut budget = 20;
            {
                let mut a = bridge.attach(&mut cpu).unwrap();
                let mut r = Routed::attach(&mut a, &router, &services).unwrap();
                assert_eq!(r.run(GuestAddr(0x400000), &mut budget).unwrap(), status);
            }
            assert_eq!((budget, cpu.base.x[0]), (remaining, x0));
        }
        let router =
            Router::new(artifacts[..3].iter().map(|a| target(&lib, a)).collect(), 16).unwrap();
        let services = Services::new(vec![]).unwrap();
        let owned = memory();
        let mut bridge = Bridge::new(owned).unwrap();
        let mut cpu = CpuV10::default();
        let mut a = bridge.attach(&mut cpu).unwrap();
        let mut r = Routed::attach(&mut a, &router, &services).unwrap();
        let preflight = r.table().preflight.unwrap();
        let ctx = r.table().guard_context;
        let call = r.table().base.base.call.unwrap();
        let svc = r.table().base.svc.unwrap();
        let frame = r.cpu_mut() as *mut CpuV10;
        let mut foreign = CpuV10::default();
        foreign.base.x[0] = 123;
        assert_eq!(
            unsafe { preflight(ctx, &mut foreign, std::ptr::dangling(), 0) },
            3
        );
        assert_eq!(foreign.base.x[0], 123);
        assert_eq!(unsafe { preflight(ctx, frame, std::ptr::dangling(), 0) }, 3);
        let mut budget = 7;
        assert_eq!(unsafe { call(ctx, frame, 0x400000, &mut budget) }, 3);
        assert_eq!(budget, 7);
        assert_eq!(unsafe { svc(ctx, frame, 0x29) }, 3);
        let previous = r.cpu_mut().base.memory;
        r.cpu_mut().base.memory = None;
        let before = r.cpu_mut().base.pc;
        assert!(r.run(GuestAddr(0x400000), &mut budget).is_err());
        assert_eq!((budget, r.cpu_mut().base.pc), (7, before));
        r.cpu_mut().base.memory = previous;
    }
}
#[derive(Default)]
struct Fake {
    preflights: u32,
    calls: u32,
}
unsafe extern "C" fn fake_preflight(
    p: *mut std::ffi::c_void,
    _: *mut CpuV10,
    _: *const u8,
    _: u64,
) -> i32 {
    unsafe { (*p.cast::<Fake>()).preflights += 1 };
    0
}
unsafe extern "C" fn fake_call(
    p: *mut std::ffi::c_void,
    _: *mut CpuV10,
    _: u64,
    _: *mut u64,
) -> i32 {
    unsafe { (*p.cast::<Fake>()).calls += 1 };
    0
}
unsafe extern "C" fn fake_svc(_: *mut std::ffi::c_void, _: *mut CpuV10, _: u32) -> i32 {
    3
}
#[test]
fn generated_entry_rejects_malformed_prefix_and_success_from_a_terminal_callback() {
    let mut w = vec![0xd503201fu32; 40];
    w[0] = 0x94000010;
    w[16..20].copy_from_slice(&[0xd0000010, 0xf9400e11, 0x91006210, 0xd61f0220]);
    w[32] = 0x14000000;
    let image = Image::new(0, bytes(&w)).unwrap();
    let loop_effect = Effect::from_local(
        &image
            .analyze(0x80, 4, &[])
            .unwrap()
            .no_return_fact()
            .unwrap(),
    );
    let alias = image
        .conditional_import(&image.import_thunk(0x40, 0).unwrap(), &loop_effect)
        .unwrap();
    let artifact = Artifact::new(
        Contract::analyze(&image, 0, 4, &[alias]).unwrap(),
        "mkw_gv4_defense",
    )
    .unwrap();
    for opt in ["-O0", "-O2"] {
        let lib = compile(std::slice::from_ref(&artifact), "emitter-defense", opt, "");
        let function = unsafe {
            lib.get::<unsafe extern "C" fn(*mut CpuV10, *mut u64) -> i32>(b"mkw_gv4_defense\0")
                .unwrap()
        };
        let owned = memory();
        let mut bridge = Bridge::new(owned).unwrap();
        for case in 0..=22 {
            let mut cpu = CpuV10::default();
            cpu.base.module_base = 0x400000;
            cpu.base.pc = 0x400000;
            cpu.base.x[0] = 55;
            cpu.base.x[30] = 0x500000;
            let mut a = bridge.attach(&mut cpu).unwrap();
            let previous = a.cpu_mut().exclusive;
            let mut prefix = unsafe { *previous };
            prefix.version = 4;
            prefix.size = 112;
            let mut fake = Fake::default();
            let ctx = std::ptr::addr_of_mut!(fake).cast();
            use mkw_runtime::{
                exclusive_calls::{OpsV2, OpsV3},
                guarded::OpsV4,
            };
            let mut table = Box::new(OpsV4 {
                base: OpsV3 {
                    base: OpsV2 {
                        base: prefix,
                        call: Some(fake_call),
                        call_context: ctx,
                    },
                    svc: Some(fake_svc),
                    svc_context: ctx,
                },
                preflight: Some(fake_preflight),
                guard_context: ctx,
            });
            a.cpu_mut().exclusive = &table.base.base.base;
            match case {
                0 => table.base.base.base.version = 3,
                1 => table.base.base.base.size = 96,
                2 => table.base.base.base.load = None,
                3 => table.base.base.base.store = None,
                4 => table.base.base.base.clear = None,
                5 => table.base.base.base.memory = None,
                6 => table.base.base.base.ordered_memory = None,
                7 => table.base.base.base.atomic_memory = None,
                8 => table.base.base.base.barrier = None,
                9 => table.base.base.call = None,
                10 => table.base.base.call_context = std::ptr::null_mut(),
                11 => table.base.svc = None,
                12 => table.base.svc_context = std::ptr::null_mut(),
                13 => table.preflight = None,
                14 => table.guard_context = std::ptr::null_mut(),
                15 => a.cpu_mut().base.memory = None,
                16 => a.cpu_mut().base.memory_context = std::ptr::null_mut(),
                17 => a.cpu_mut().base.call_context = ctx,
                18 => a.cpu_mut().exclusive_context = std::ptr::null_mut(),
                19 => a.cpu_mut().base.pc += 4,
                20 => a.cpu_mut().exclusive = std::ptr::null(),
                _ => {}
            }
            let mut budget = 10;
            let status = unsafe {
                function(
                    a.cpu_mut(),
                    if case == 21 {
                        std::ptr::null_mut()
                    } else {
                        &mut budget
                    },
                )
            };
            if case <= 21 {
                assert_eq!((status, budget, fake.preflights, fake.calls), (3, 10, 0, 0));
                assert_eq!(a.cpu_mut().base.x[30], 0x500000);
            } else {
                assert_eq!((status, budget, fake.preflights, fake.calls), (6, 9, 1, 1));
                assert_eq!(a.cpu_mut().base.fault_status, 6);
                assert_eq!(a.cpu_mut().base.x[30], 0x400004);
            }
            assert_eq!(a.cpu_mut().base.x[0], 55);
            a.cpu_mut().exclusive = previous;
        }
    }
}
