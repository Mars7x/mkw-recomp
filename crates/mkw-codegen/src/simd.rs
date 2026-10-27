use super::{r, write};
use mkw_arm64::simd::{ImmediateKind, LaneSource, Simd, TransformKind};
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
        Simd::Transform{dst,src,bytes,vector_bytes,kind}=>{
            let mut out=format!("{{uint64_t source[2]={{c->vregs[{src}][0],c->vregs[{src}][1]}},output[2]={{0,0}};\n");
            match kind{
                TransformKind::Not=>out+=&format!("for(unsigned half=0;half<{};half++)output[half]=~source[half];\n",vector_bytes/8),
                TransformKind::ReverseBits=>out+=&format!("for(unsigned i=0;i<{vector_bytes};i++){{uint64_t value=(source[i/8]>>((i%8)*8))&UINT64_C(255),reverse=0;for(unsigned bit=0;bit<8;bit++)reverse|=((value>>bit)&UINT64_C(1))<<(7-bit);output[i/8]|=reverse<<((i%8)*8);}}\n"),
                TransformKind::ReverseElements(block)=>out+=&format!("for(unsigned i=0;i<{vector_bytes};i+={bytes}){{unsigned at=(i/{block})*{block}+{block}-{bytes}-(i%{block});uint64_t value=(source[at/8]>>((at%8)*8))&UINT64_C({});output[i/8]|=value<<((i%8)*8);}}\n",mask(bytes)),
            }
            out+=&format!("c->vregs[{dst}][0]=output[0];c->vregs[{dst}][1]=output[1];}}\n");out
        }
        Simd::Table {dst,base,count,index,bytes,extension}=>{
            let mut out=format!("{{uint64_t table[{count}][2],index[2]={{c->vregs[{index}][0],c->vregs[{index}][1]}},old[2]={{c->vregs[{dst}][0],c->vregs[{dst}][1]}},output[2]={{0,0}};(void)old;\n");
            for slot in 0..count {let reg=(base+slot)%32;out+=&format!("table[{slot}][0]=c->vregs[{reg}][0];table[{slot}][1]=c->vregs[{reg}][1];\n");}
            out+=&format!("for(unsigned i=0;i<{bytes};i++){{unsigned at=(unsigned)((index[i/8]>>((i%8)*8))&UINT64_C(255));uint64_t value;if(at<{})value=(table[at/16][(at%16)/8]>>((at%8)*8))&UINT64_C(255);else value={};output[i/8]|=value<<((i%8)*8);}}c->vregs[{dst}][0]=output[0];c->vregs[{dst}][1]=output[1];}}\n",count*16,if extension {"(old[i/8]>>((i%8)*8))&UINT64_C(255)"}else{"UINT64_C(0)"});out
        }
        Simd::ByteCount {dst,src,bytes}=>format!("{{uint64_t source[2]={{c->vregs[{src}][0],c->vregs[{src}][1]}},output[2]={{0,0}};for(unsigned i=0;i<{bytes};i++){{uint64_t value=(source[i/8]>>((i%8)*8))&UINT64_C(255),count=0;for(unsigned bit=0;bit<8;bit++)count+=(value>>bit)&UINT64_C(1);output[i/8]|=count<<((i%8)*8);}}c->vregs[{dst}][0]=output[0];c->vregs[{dst}][1]=output[1];}}\n"),
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
        Simd::Permute {dst,lhs,rhs,bytes,vector_bytes,kind,part}=>{
            let count=vector_bytes/bytes;
            let (bank,index)=match kind {
                0=>(format!("i/{}",count/2),format!("2*(i%{})+{part}",count/2)),
                1=>("i%2".into(),format!("2*(i/2)+{part}")),
                2=>("i%2".into(),format!("{}+i/2",part*count/2)),_=>unreachable!(),
            };
            format!("{{uint64_t source[2][2]={{{{c->vregs[{lhs}][0],c->vregs[{lhs}][1]}},{{c->vregs[{rhs}][0],c->vregs[{rhs}][1]}}}},out[2]={{0,0}};for(unsigned i=0;i<{count};i++){{unsigned bank={bank},index={index},offset=index*{};uint64_t value=(source[bank][offset/64]>>(offset%64))&UINT64_C({});unsigned to=i*{};out[to/64]|=value<<(to%64);}}c->vregs[{dst}][0]=out[0];c->vregs[{dst}][1]=out[1];}}\n",bytes*8,mask(bytes),bytes*8)
        }
        Simd::ExtractVector{dst,lhs,rhs,bytes,offset}=>format!("{{uint64_t out[2]={{0,0}};for(unsigned i=0;i<{bytes};i++){{unsigned at=i+{offset};uint64_t value;if(at<{bytes})value=c->vregs[{lhs}][at/8];else{{at-={bytes};value=c->vregs[{rhs}][at/8];}}out[i/8]|=((value>>((at%8)*8))&UINT64_C(255))<<((i%8)*8);}}c->vregs[{dst}][0]=out[0];c->vregs[{dst}][1]=out[1];}}\n"),
    }
}
