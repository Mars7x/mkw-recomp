use mkw_arm64::lift;
use mkw_codegen::emit;
#[test]
fn native_authenticated_stack_return_and_failure_paths() {
    // Independently authored PACIBSP; STP FP,LR,[SP,-16]!; MOV FP,SP;
    // MOV W0,42; LDP FP,LR,[SP],16; RETAB. No game-derived fixture.
    let words = [
        0xd503237fu32,
        0xa9bf7bfd,
        0x910003fd,
        0x52800540,
        0xa8c17bfd,
        0xd65f0fff,
    ];
    let bytes: Vec<_> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
    let code = emit(&lift(&bytes, 0x8000, 0x8000).unwrap(), 0x8000).unwrap();
    let harness = r#"
#include "pac.h"
#include <string.h>
typedef struct {uint8_t data[256];int tamper;} Stack;
static int memory(void *ctx,int store,uint64_t addr,uint64_t *values,unsigned count){
 Stack *s=ctx;if(count!=16||addr<0x1000||addr-0x1000>240)return 5;
 for(unsigned word=0;word<2;word++){
  if(!store)values[word]=0;
  for(unsigned b=0;b<8;b++){
   size_t index=(size_t)(addr-0x1000)+word*8+b;
   if(store)s->data[index]=(uint8_t)(values[word]>>(b*8));
   else values[word]|=(uint64_t)s->data[index]<<(b*8);
  }
 }
 if(!store&&s->tamper)values[1]^=UINT64_C(1)<<60;
 return 0;
}
int main(void){
 MkwPac39 keys={{1,3},{2,4}};Stack stack={{0},0};
 MkwCpu c={0};c.sp=0x1100;c.x[29]=99;c.x[30]=0x2000;c.pauth=mkw_pac39;c.pac_context=&keys;c.memory=memory;c.memory_context=&stack;
 if(mkw_guest(&c,20)||c.x[0]!=42||c.sp!=0x1100||c.x[29]!=99||c.pc!=0x2000||c.x[30]==0x2000)return 1;
 c=(MkwCpu){0};c.x[30]=0x2000;if(mkw_guest(&c,20)!=3||c.pc!=0x8000||c.x[30]!=0x2000)return 2;
 c=(MkwCpu){0};c.sp=0x1000;c.x[30]=0x2000;c.pauth=mkw_pac39;c.pac_context=&keys;c.memory=memory;c.memory_context=&stack;
 if(mkw_guest(&c,20)!=5||c.pc!=0x8004||c.sp!=0x1000||c.fault_address!=0xff0)return 3;
 stack.tamper=1;c=(MkwCpu){0};c.sp=0x1100;c.x[30]=0x2000;c.pauth=mkw_pac39;c.pac_context=&keys;c.memory=memory;c.memory_context=&stack;
 if(mkw_guest(&c,20)!=4||c.pc!=0x8014)return 4;
 uint64_t a,b,result;
 if(mkw_pac39(&keys,0,0,0x2000,0x1100,&a)||mkw_pac39(&keys,0,1,0x2000,0x1100,&b)||a==b)return 5;
 if(mkw_pac39(&keys,1,1,b,0x1100,&result)||result!=0x2000)return 6;
 if(mkw_pac39(&keys,3,1,b,0x1110,&result)!=4)return 7;
 if(mkw_pac39(&keys,2,1,b,0,&result)||result!=0x2000)return 8;
 if(mkw_pac39(&keys,0,1,UINT64_C(0xffffff8000002000),0x1100,&b)||mkw_pac39(&keys,1,1,b,0x1100,&result)||result!=UINT64_C(0xffffff8000002000))return 9;
 return 0;
}
"#;
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let folder = root.join("private/testing/pac-native");
    std::fs::create_dir_all(&folder).unwrap();
    let src = folder.join("test.c");
    let exe = folder.join("test");
    std::fs::write(&src, code + harness).unwrap();
    for optimization in ["-O0", "-O2"] {
        let built = std::process::Command::new("cc")
            .args([
                "-std=c11",
                optimization,
                "-Wall",
                "-Wextra",
                "-Werror",
                "-fsanitize=undefined",
                "-fno-sanitize-recover=undefined",
            ])
            .arg("-I")
            .arg(root.join("native"))
            .arg(&src)
            .arg(root.join("native/pac.c"))
            .arg(root.join("third_party/gem5-qarma/qarma.c"))
            .arg("-o")
            .arg(&exe)
            .output()
            .unwrap();
        assert!(
            built.status.success(),
            "{}",
            String::from_utf8_lossy(&built.stderr)
        );
        let run = std::process::Command::new(&exe).output().unwrap();
        assert!(
            run.status.success(),
            "status {:?}: {}",
            run.status.code(),
            String::from_utf8_lossy(&run.stderr)
        );
    }
}

#[test]
fn native_calls_share_budget_and_preserve_authenticated_return() {
    let caller = [
        0xd503237fu32,
        0xa9be7bfd,
        0x910003fd,
        0xd2800ee0,
        0xf9000be0,
        0xb94013e1,
        0x940003fa,
        0xa8c27bfd,
        0xd65f0fff,
    ];
    let callee = [
        0xd503233fu32,
        0xd10043ff,
        0x11000420,
        0x910043ff,
        0xd65f0bff,
    ];
    let compile_ir = |words: &[u32], base| {
        let bytes: Vec<_> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
        emit(&lift(&bytes, base, base).unwrap(), base).unwrap()
    };
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let folder = root.join("private/testing/pac-calls");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(
        folder.join("callee.c"),
        compile_ir(&callee, 0x9000).replace("mkw_guest", "callee"),
    )
    .unwrap();
    let harness = r#"
#include "pac.h"
int callee_budget(MkwCpu*,uint64_t*);
typedef struct{uint8_t bytes[256];} Stack;
static int memory(void *ctx,int store,uint64_t address,uint64_t *values,unsigned bytes){
 if(!bytes||bytes>16||address<0x1000||address-0x1000>256-bytes)return 5;
 Stack *s=ctx;if(!store){values[0]=0;values[1]=0;}
 for(unsigned i=0;i<bytes;i++){
  if(store)s->bytes[address-0x1000+i]=(uint8_t)(values[i/8]>>((i%8)*8));
  else values[i/8]|=(uint64_t)s->bytes[address-0x1000+i]<<((i%8)*8);
 }return 0;
}
static int call(void *ctx,MkwCpu *c,uint64_t target,uint64_t *budget){(void)ctx;return target==0x9000?callee_budget(c,budget):3;}
static int wrong_return(void *ctx,MkwCpu *c,uint64_t target,uint64_t *budget){(void)ctx;(void)target;(void)budget;c->pc=1;return 0;}
int main(void){MkwPac39 keys={{1,3},{2,4}};Stack stack={{0}};MkwCpu original={0};original.sp=0x1100;original.x[29]=99;original.x[1]=UINT64_MAX;original.x[30]=0x2000;original.pauth=mkw_pac39;original.pac_context=&keys;original.memory=memory;original.memory_context=&stack;original.call=call;
 MkwCpu c=original;if(mkw_guest(&c,100)||c.x[0]!=120||c.x[1]!=119||c.pc!=0x2000||c.sp!=0x1100||c.x[29]!=99)return 1;
 c=original;if(mkw_guest(&c,10)!=2||c.pc!=0x900c||c.sp!=0x10d0)return 2;
 c=original;c.call=0;if(mkw_guest(&c,100)!=3||c.pc!=0x8018||c.fault_address!=0x9000)return 3;
 c=original;c.call=wrong_return;if(mkw_guest(&c,100)!=6||c.pc!=1)return 4;
 return 0;}
"#;
    std::fs::write(
        folder.join("caller.c"),
        compile_ir(&caller, 0x8000) + harness,
    )
    .unwrap();
    for optimization in ["-O0", "-O2"] {
        let result = std::process::Command::new("cc")
            .args([
                "-std=c11",
                optimization,
                "-Wall",
                "-Wextra",
                "-Werror",
                "-fsanitize=undefined",
                "-fno-sanitize-recover=undefined",
            ])
            .arg("-I")
            .arg(root.join("native"))
            .arg(folder.join("caller.c"))
            .arg(folder.join("callee.c"))
            .arg(root.join("native/pac.c"))
            .arg(root.join("third_party/gem5-qarma/qarma.c"))
            .arg("-o")
            .arg(folder.join("test"))
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let run = std::process::Command::new(folder.join("test"))
            .output()
            .unwrap();
        assert!(
            run.status.success(),
            "status {:?}: {}",
            run.status.code(),
            String::from_utf8_lossy(&run.stderr)
        );
    }
}
