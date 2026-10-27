use super::{r, write};
use mkw_arm64::simd::{CryptoKind, ImmediateKind, LaneSource, Simd, TransformKind};
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
        Simd::Crypto{dst,src,kind}=>emit_crypto(dst,src,kind),
        Simd::Narrow{dst,src,bytes,upper}=>{
            let bits=bytes*8;let input_bits=bits*2;
            format!("{{uint64_t source[2]={{c->vregs[{src}][0],c->vregs[{src}][1]}},value=0;for(unsigned lane=0;lane<{};lane++){{unsigned from=lane*{input_bits};value|=((source[from/64]>>(from%64))&UINT64_C({}))<<(lane*{bits});}}c->vregs[{dst}][{}]=value;{}}}\n",8/bytes,mask(bytes),u8::from(upper),if upper {String::new()} else {format!("c->vregs[{dst}][1]=0;")})
        }

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

// Original finite-field construction; algorithm definitions follow NIST FIPS 197.
fn aes_product(mut a: u8, mut b: u8) -> u8 {
    let mut out = 0;
    for _ in 0..8 {
        if b & 1 != 0 {
            out ^= a;
        }
        a = a.wrapping_shl(1) ^ if a & 0x80 != 0 { 0x1b } else { 0 };
        b >>= 1;
    }
    out
}
fn aes_substitution() -> [u8; 256] {
    std::array::from_fn(|n| {
        let mut inverse = 1;
        let mut power = n as u8;
        for bit in 0..8 {
            if 254 & (1 << bit) != 0 {
                inverse = aes_product(inverse, power);
            }
            power = aes_product(power, power);
        }
        inverse
            ^ inverse.rotate_left(1)
            ^ inverse.rotate_left(2)
            ^ inverse.rotate_left(3)
            ^ inverse.rotate_left(4)
            ^ 0x63
    })
}
fn emit_crypto(dst: u8, src: u8, kind: CryptoKind) -> String {
    let mut out = String::from("{uint8_t result[16]={0};\n");
    match kind {
        CryptoKind::EncryptRound | CryptoKind::DecryptRound => {
            let inverse = kind == CryptoKind::DecryptRound;
            let forward = aes_substitution();
            let table = if inverse {
                let mut reverse = [0; 256];
                for (n, &v) in forward.iter().enumerate() {
                    reverse[usize::from(v)] = n as u8;
                }
                reverse
            } else {
                forward
            };
            let values = table
                .iter()
                .map(u8::to_string)
                .collect::<Vec<_>>()
                .join(",");
            out += &format!("static const uint8_t substitute[256]={{{values}}};\n");
            out+=&format!("uint8_t state[16]={{0}},key[16]={{0}};for(unsigned i=0;i<16;i++){{state[i]=(uint8_t)(c->vregs[{dst}][i/8]>>((i%8)*8));key[i]=(uint8_t)(c->vregs[{src}][i/8]>>((i%8)*8));}}\n");
            out+=&format!("for(unsigned column=0;column<4;column++)for(unsigned row=0;row<4;row++){{unsigned from=row+4*((column+{})&3);result[row+4*column]=substitute[state[from]^key[from]];}}\n",if inverse{"4-row"}else{"row"});
        }
        CryptoKind::MixColumns | CryptoKind::InverseMixColumns => {
            let inverse = kind == CryptoKind::InverseMixColumns;
            out+=&format!("uint8_t source[16]={{0}};for(unsigned i=0;i<16;i++)source[i]=(uint8_t)(c->vregs[{src}][i/8]>>((i%8)*8));\n");
            out += "for(unsigned column=0;column<4;column++){uint8_t a[4],x2[4];";
            if inverse {
                out += "uint8_t x4[4],x8[4];";
            }
            out+="for(unsigned i=0;i<4;i++){a[i]=source[column*4+i];x2[i]=(uint8_t)((a[i]<<1)^((a[i]>>7)*27));";
            if inverse {
                out+="x4[i]=(uint8_t)((x2[i]<<1)^((x2[i]>>7)*27));x8[i]=(uint8_t)((x4[i]<<1)^((x4[i]>>7)*27));";
            }
            out += "}for(unsigned row=0;row<4;row++){result[row+4*column]=";
            if inverse {
                out+="x2[row]^x4[row]^x8[row]^a[(row+1)&3]^x2[(row+1)&3]^x8[(row+1)&3]^a[(row+2)&3]^x4[(row+2)&3]^x8[(row+2)&3]^a[(row+3)&3]^x8[(row+3)&3];";
            } else {
                out += "x2[row]^x2[(row+1)&3]^a[(row+1)&3]^a[(row+2)&3]^a[(row+3)&3];";
            }
            out += "}}\n";
        }
    }
    out+=&format!("uint64_t packed[2]={{0,0}};for(unsigned i=0;i<16;i++)packed[i/8]|=(uint64_t)result[i]<<((i%8)*8);c->vregs[{dst}][0]=packed[0];c->vregs[{dst}][1]=packed[1];}}\n");
    out
}
#[cfg(test)]
mod aes_construction_tests {
    use super::*;
    #[test]
    fn derived_substitution_is_bijective_and_has_standard_cells() {
        let s = aes_substitution();
        assert_eq!((s[0], s[0x53], s[0x7c], s[255]), (0x63, 0xed, 0x10, 0x16));
        let mut values = s;
        values.sort();
        assert_eq!(values, std::array::from_fn(|n| n as u8));
    }
}
