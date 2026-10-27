use mkw_runtime::{
    abi::Cpu,
    dispatch::{NativeDispatcher, NativeTarget},
    GuestAddr,
};
use std::path::Path;
#[test]
fn native_br_local_external_tail_chains_cycles_and_refusals() {
    let mut code = mkw_codegen::HEADER.to_string();
    for (name, words, function) in [
        ("root", vec![0xd61f0120u32], false), // BR X9, no fallthrough
        ("tail", vec![0x14000400], true),     // B +0x1000 outside the exact function extent
        ("finish", vec![0x91000400, 0xd65f03c0], false),
        ("cycle", vec![0xd61f0120], false),
        (
            "caller",
            vec![0xaa1e03f4, 0xd63f0120, 0xaa1403fe, 0xd65f03c0],
            false,
        ),
        (
            "local",
            vec![0xb4000040, 0xd61f0120, 0x91000421, 0xd65f03c0],
            false,
        ),
        ("fault", vec![0xf9400020, 0xd65f03c0], false),
    ] {
        let bytes: Vec<_> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
        let ir = if function {
            mkw_arm64::lift_function(&bytes, 0, 0)
        } else {
            mkw_arm64::lift(&bytes, 0, 0)
        }
        .unwrap();
        code += &mkw_codegen::emit(&ir, 0)
            .unwrap()
            .replace(mkw_codegen::HEADER, "")
            .replace("mkw_guest", name);
    }
    code += "int bad_budget(MkwCpu*c,uint64_t*budget){(void)budget;c->pc=0xd000;return MKW_EXIT_TAIL;}\n";
    let folder = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../private/testing/tail-routing");
    std::fs::create_dir_all(&folder).unwrap();
    let src = folder.join("test.c");
    std::fs::write(&src, code).unwrap();
    for opt in ["-O0", "-O2"] {
        let output = folder.join(format!("entries{}.so", &opt[1..]));
        let build = std::process::Command::new("cc")
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
            ])
            .arg(&src)
            .arg("-o")
            .arg(&output)
            .output()
            .unwrap();
        assert!(
            build.status.success(),
            "{}",
            String::from_utf8_lossy(&build.stderr)
        );
        // SAFETY: locally compiled synthetic functions implement ABI v5. Library
        // outlives every symbol/attachment and callbacks are installed by guards.
        unsafe {
            let library = libloading::Library::new(&output).unwrap();
            type Entry = unsafe extern "C" fn(*mut Cpu, u64) -> i32;
            let root = library.get::<Entry>(b"root\0").unwrap();
            let caller = library.get::<Entry>(b"caller\0").unwrap();
            let local = library.get::<Entry>(b"local\0").unwrap();
            let targets = [
                (b"tail_budget\0".as_slice(), 0x9000),
                (b"finish_budget\0", 0xa000),
                (b"cycle_budget\0", 0xb000),
                (b"fault_budget\0", 0xc000),
                (b"bad_budget\0", 0xd000),
            ]
            .into_iter()
            .map(|(name, at)| NativeTarget::load(&library, name, GuestAddr(at), at).unwrap())
            .collect();
            let dispatcher = NativeDispatcher::new(targets).unwrap();
            for entry in [*root, *caller] {
                let mut cpu = Cpu {
                    module_base: 0x8000,
                    ..Cpu::default()
                };
                cpu.x[9] = 0x9000;
                cpu.x[30] = 0xdead;
                {
                    let mut calls = dispatcher.as_ref().attach(&mut cpu);
                    assert_eq!(entry(calls.cpu_mut(), 100), 0);
                }
                assert_eq!(
                    (cpu.x[0], cpu.x[30], cpu.pc, cpu.module_base),
                    (1, 0xdead, 0xdead, 0x8000)
                );
            }
            let mut cpu = Cpu {
                module_base: 0x8000,
                ..Cpu::default()
            };
            cpu.x[0] = 1;
            cpu.x[9] = 0x8008;
            cpu.x[30] = 0xdead;
            assert_eq!(local(&mut cpu, 100), 0);
            assert_eq!((cpu.x[1], cpu.pc, cpu.x[30]), (1, 0xdead, 0xdead));
            cpu.x[9] = 0x8004;
            assert_eq!(local(&mut cpu, 20), 2);
            assert_eq!(cpu.pc, 0x8004);
            for target in [0xdead, 0x9001, 0, 0xc000, 0xb000, 0xd000] {
                let mut cpu = Cpu {
                    module_base: 0x8000,
                    ..Cpu::default()
                };
                cpu.x[9] = target;
                cpu.x[30] = 0xdead;
                let mut calls = dispatcher.as_ref().attach(&mut cpu);
                assert_eq!(
                    root(calls.cpu_mut(), 100_000),
                    if target == 0xb000 {
                        2
                    } else if target == 0xd000 {
                        6
                    } else {
                        3
                    }
                );
                assert_eq!(calls.cpu_mut().module_base, 0x8000);
                assert_eq!(calls.cpu_mut().x[30], 0xdead);
                if target != 0xb000 && target != 0xc000 {
                    assert_eq!(calls.cpu_mut().fault_address, target);
                }
            }
            let mut cpu = Cpu {
                module_base: 0x8000,
                ..Cpu::default()
            };
            cpu.x[9] = 0x9000;
            assert_eq!(root(&mut cpu, 100), 3);
            assert_eq!(cpu.fault_address, 0x9000);
            let mut calls = dispatcher.as_ref().attach(&mut cpu);
            assert_eq!(root(calls.cpu_mut(), 0), 2);
            assert_eq!(calls.cpu_mut().pc, 0x8000);
        }
    }
}
