use super::{r, write};
use mkw_arm64::{floating::SystemReg, Op, Width};
pub fn emit(op: Op) -> String {
    match op {
        Op::Convert(op)=>{
            let input=if op.operation<2{r(op.int_reg)}else{format!("c->vregs[{}][0]",op.vector)};
            let operation=u32::from(op.operation)|if op.fractional_bits==0{0}else{mkw_arm64::floating::CONVERT_FIXED_SCALE|(u32::from(op.fractional_bits)<<mkw_arm64::floating::CONVERT_SCALE_SHIFT)};
            let bits=if op.int_width==Width::X64{64}else{32};
            let mut out=format!("{{uint64_t result=0;uint32_t fpsr=c->fpsr;if(!c->convert){{c->fault_status=3;return 3;}}int status=c->convert({},{bits},{},c->fpcr,c->fpsr,{input},&result,&fpsr);if(status){{c->fault_status=status;return status;}}c->fpsr=fpsr;\n",operation,op.bytes);
            if op.operation<2{out+=&format!("c->vregs[{}][0]=result;c->vregs[{}][1]=0;\n",op.vector,op.vector);}else{out+=&write(op.int_reg,op.int_width,"result");}
            out+="}\n";out
        }
        Op::Floating(op)=>{
            let accumulate=if matches!(op.operation,6|11|12|13){format!("c->vregs[{}][0]",op.accumulate)}else{"UINT64_C(0)".into()};
            let right=if op.zero{"UINT64_C(0)".into()}else{format!("c->vregs[{}][0]",op.rhs)};
            let mut out=format!("{{uint64_t result=0;uint32_t fpsr=c->fpsr;if(!c->floating){{c->fault_status=3;return 3;}}int status=c->floating({},{},c->fpcr,c->fpsr,c->vregs[{}][0],{right},{accumulate},&result,&fpsr);if(status){{c->fault_status=status;return status;}}c->fpsr=fpsr;\n",op.operation,op.bytes,op.lhs);
            if matches!(op.operation,4|5){out+="c->n=(uint32_t)((result>>3)&1);c->z=(uint32_t)((result>>2)&1);c->c=(uint32_t)((result>>1)&1);c->v=(uint32_t)(result&1);\n";}
            else {out+=&format!("c->vregs[{}][0]=result;c->vregs[{}][1]=0;\n",op.dst,op.dst);}
            out+="}\n";out
        }
        Op::SystemRegister{read,register:register @ (SystemReg::TpidrEl0|SystemReg::TpidrroEl0),value}=>{
            let field=if register==SystemReg::TpidrEl0{"tpidr_el0"}else{"tpidrro_el0"};
            let mut out=String::from("if(c->thread_state_configured!=1){c->fault_status=3;return 3;}\n");
            if read{out+=&write(value,Width::X64,&format!("c->{field}"));}
            else if register==SystemReg::TpidrEl0{out+=&format!("c->{field}={};\n",r(value));}
            else{out+="c->fault_status=3;return 3;\n";}
            out
        }
        Op::SystemRegister{read,register,value}=>{
            if read {let expression=match register{SystemReg::Fpcr=>"c->fpcr",SystemReg::Fpsr=>"c->fpsr",SystemReg::TpidrEl0|SystemReg::TpidrroEl0=>unreachable!("handled thread register"),SystemReg::Nzcv=>"((uint64_t)c->n<<31)|((uint64_t)c->z<<30)|((uint64_t)c->c<<29)|((uint64_t)c->v<<28)"};write(value,Width::X64,expression)}else{
                let src=r(value);match register{
                    SystemReg::Fpcr=>format!("{{uint64_t value={src};if(value&~UINT64_C(0x07c89f00)){{c->fault_status=3;return 3;}}c->fpcr=(uint32_t)value;}}\n"),
                    SystemReg::Fpsr=>format!("c->fpsr=(uint32_t)({src}&UINT64_C(0x0800009f));\n"),
                    SystemReg::TpidrEl0|SystemReg::TpidrroEl0=>unreachable!("handled thread register"),
                    SystemReg::Nzcv=>format!("{{uint64_t value={src};c->n=(uint32_t)((value>>31)&1);c->z=(uint32_t)((value>>30)&1);c->c=(uint32_t)((value>>29)&1);c->v=(uint32_t)((value>>28)&1);}}\n"),
                }
            }
        }
        Op::Barrier{kind,option}=>format!("if(!c->barrier){{c->fault_status=3;return 3;}}{{int status=c->barrier(c->memory_context,{kind},{option});if(status){{c->fault_status=status;return status;}}}}\n"),
        _=>unreachable!(),
    }
}
