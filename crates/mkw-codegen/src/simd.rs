use super::{r, write};
use mkw_arm64::simd::{ImmediateKind, LaneSource, Simd};
fn mask(bytes: u8) -> u64 {
    if bytes == 8 {
        u64::MAX
    } else {
        (1u64 << (bytes * 8)) - 1
    }
}
fn lane(source: LaneSource, bytes: u8) -> String {
    let expr = match source {
        LaneSource::General(reg) => r(reg),
        LaneSource::Vector { register, lane } => {
            let bit = lane * bytes * 8;
            format!("(c->vregs[{register}][{}]>>{})", bit / 64, bit % 64)
        }
    };
    format!("({expr}&UINT64_C({}))", mask(bytes))
}
pub fn emit(op: Simd) -> String {
    match op {
        Simd::Immediate{dst,value,bytes,kind}=>{
            let value=if bytes<8{value&mask(bytes)}else{value};
            let expression=|half|match kind {ImmediateKind::Move=>format!("UINT64_C({value})"),ImmediateKind::Or=>format!("c->vregs[{dst}][{half}]|UINT64_C({value})"),ImmediateKind::Bic=>format!("c->vregs[{dst}][{half}]&~UINT64_C({value})")};
            format!("c->vregs[{dst}][0]={};c->vregs[{dst}][1]={};\n",expression(0),if bytes==16{expression(1)}else{"0".into()})
        }
        Simd::Scalar{dst,bytes,source}=>format!("{{uint64_t value={};c->vregs[{dst}][0]=value;c->vregs[{dst}][1]=0;}}\n",lane(source,bytes)),
        Simd::Duplicate{dst,bytes,vector_bytes,source}=>format!("{{uint64_t value={},out[2]={{0,0}};for(unsigned b=0;b<{vector_bytes};b+={bytes})out[b/8]|=value<<((b%8)*8);c->vregs[{dst}][0]=out[0];c->vregs[{dst}][1]=out[1];}}\n",lane(source,bytes)),
        Simd::Insert{dst,lane:at,bytes,source}=>{let bit=at*bytes*8;format!("{{uint64_t value={};c->vregs[{dst}][{}]=(c->vregs[{dst}][{}]&~UINT64_C({}))|(value<<{});}}\n",lane(source,bytes),bit/64,bit/64,mask(bytes)<<(bit%64),bit%64)},
        Simd::Extract{dst,width,src,lane:at,bytes,signed}=>{
            let expression=lane(LaneSource::Vector{register:src,lane:at},bytes);
            let expression=if signed{let sign=1u64<<(bytes*8-1);format!("(({expression}^UINT64_C({sign}))-UINT64_C({sign}))")}else{expression};
            write(dst,width,&expression)
        }
        Simd::Logical{dst,lhs,rhs,bytes,operation}=>{
            let expr=match operation {0=>"a&b",1=>"a&~b",2=>"a|b",3=>"a|~b",4=>"a^b",5=>"(d&a)|(~d&b)",6=>"(d&~b)|(a&b)",7=>"(d&b)|(a&~b)",_=>unreachable!()};
            format!("{{uint64_t out[2]={{0,0}};for(unsigned h=0;h<{};h++){{uint64_t a=c->vregs[{lhs}][h],b=c->vregs[{rhs}][h],d=c->vregs[{dst}][h];(void)d;out[h]={expr};}}c->vregs[{dst}][0]=out[0];c->vregs[{dst}][1]=out[1];}}\n",bytes/8)
        }
        Simd::ExtractVector{dst,lhs,rhs,bytes,offset}=>format!("{{uint64_t out[2]={{0,0}};for(unsigned i=0;i<{bytes};i++){{unsigned at=i+{offset};uint64_t value;if(at<{bytes})value=c->vregs[{lhs}][at/8];else{{at-={bytes};value=c->vregs[{rhs}][at/8];}}out[i/8]|=((value>>((at%8)*8))&UINT64_C(255))<<((i%8)*8);}}c->vregs[{dst}][0]=out[0];c->vregs[{dst}][1]=out[1];}}\n"),
    }
}
