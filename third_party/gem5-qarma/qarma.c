// -*- mode:c++ -*-

// Copyright (c) 2020 Metempsy Technology Consulting
// All rights reserved
//
// The license below extends only to copyright in the software and shall
// not be construed as granting a license to any other intellectual
// property including but not limited to intellectual property relating
// to a hardware implementation of the functionality of the software
// licensed hereunder.  You may use the software subject to the license
// terms below provided that you ensure that this notice is replicated
// unmodified and in its entirety in all distributions of the software,
// modified or unmodified, in source code or in binary form.
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions are
// met: redistributions of source code must retain the above copyright
// notice, this list of conditions and the following disclaimer;
// redistributions in binary form must reproduce the above copyright
// notice, this list of conditions and the following disclaimer in the
// documentation and/or other materials provided with the distribution;
// neither the name of the copyright holders nor the names of its
// contributors may be used to endorse or promote products derived from
// this software without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS
// "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT
// LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR
// A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT
// OWNER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
// SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT
// LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE,
// DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY
// THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT
// (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
// OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

// Adapted for this project: standalone C and uint64_t cells; algorithm unchanged.
#include "qarma.h"








static uint64_t getcell(uint64_t value, unsigned index) { return (value >> (index*4)) & 15; }
static void setcell(uint64_t *value, unsigned index, uint64_t cell) {
    const uint64_t mask=UINT64_C(15) << (index*4);
    *value=(*value & ~mask) | ((cell & 15) << (index*4));
}


uint8_t
mkw_qarma_rotCell(uint8_t incell, int amount)
{
    uint8_t res =  ((incell << amount) | (incell >> (4-amount)))& 0xF;
    return res;
}

uint8_t
mkw_qarma_tweakCellInvRot(uint8_t incell)
{
    uint8_t outcell = 0x0;
    outcell = incell << 1;
    uint8_t t = 0x1 & (incell ^ (incell>>3));
    outcell |= t;
    return outcell & 0xF;
}

uint8_t
mkw_qarma_tweakCellRot(uint8_t incell)
{
    uint8_t outcell = 0x0;
    outcell = incell >> 1;
    uint8_t t = 0x1 & (incell ^ (incell>>1));
    outcell |= t<<3;
    return outcell & 0xF;
}

BIT64
mkw_qarma_tweakInvShuffle(BIT64 indata)
{
    BIT64 outdata = 0x0;
    setcell(&outdata, 0, mkw_qarma_tweakCellInvRot(getcell(indata, 12)));
    setcell(&outdata, 1, getcell(indata, 13));
    setcell(&outdata, 2, getcell(indata, 5));
    setcell(&outdata, 3, getcell(indata, 6));
    setcell(&outdata, 4, getcell(indata, 0));
    setcell(&outdata, 5, getcell(indata, 1));
    setcell(&outdata, 6, mkw_qarma_tweakCellInvRot(getcell(indata, 2)));
    setcell(&outdata, 7, getcell(indata, 3));
    setcell(&outdata, 8, mkw_qarma_tweakCellInvRot(getcell(indata, 7)));
    setcell(&outdata, 9, mkw_qarma_tweakCellInvRot(getcell(indata, 15)));
    setcell(&outdata, 10, mkw_qarma_tweakCellInvRot(getcell(indata, 14)));
    setcell(&outdata, 11, mkw_qarma_tweakCellInvRot(getcell(indata, 4)));
    setcell(&outdata, 12, getcell(indata, 8));
    setcell(&outdata, 13, getcell(indata, 9));
    setcell(&outdata, 14, getcell(indata, 10));
    setcell(&outdata, 15, mkw_qarma_tweakCellInvRot(getcell(indata, 11)));
    return outdata;
}

BIT64
mkw_qarma_tweakShuffle(BIT64 indata)
{
    BIT64 outdata = 0x0;
    setcell(&outdata, 0, getcell(indata, 4));
    setcell(&outdata, 1, getcell(indata, 5));
    setcell(&outdata, 2, mkw_qarma_tweakCellRot(getcell(indata, 6)));
    setcell(&outdata, 3, getcell(indata, 7));
    setcell(&outdata, 4, mkw_qarma_tweakCellRot(getcell(indata, 11)));
    setcell(&outdata, 5, getcell(indata, 2));
    setcell(&outdata, 6, getcell(indata, 3));
    setcell(&outdata, 7, mkw_qarma_tweakCellRot(getcell(indata, 8)));
    setcell(&outdata, 8, getcell(indata, 12));
    setcell(&outdata, 9, getcell(indata, 13));
    setcell(&outdata, 10, getcell(indata, 14));
    setcell(&outdata, 11, mkw_qarma_tweakCellRot(getcell(indata, 15)));
    setcell(&outdata, 12, mkw_qarma_tweakCellRot(getcell(indata, 0)));
    setcell(&outdata, 13, getcell(indata, 1));
    setcell(&outdata, 14, mkw_qarma_tweakCellRot(getcell(indata, 10)));
    setcell(&outdata, 15, mkw_qarma_tweakCellRot(getcell(indata, 9)));
    return outdata;
}


BIT64
mkw_qarma_PACCellInvShuffle(BIT64 indata)
{
    BIT64 outdata = 0x0;
    setcell(&outdata, 0, getcell(indata, 3));
    setcell(&outdata, 1, getcell(indata, 6));
    setcell(&outdata, 2, getcell(indata, 12));
    setcell(&outdata, 3, getcell(indata, 9));
    setcell(&outdata, 4, getcell(indata, 14));
    setcell(&outdata, 5, getcell(indata, 11));
    setcell(&outdata, 6, getcell(indata, 1));
    setcell(&outdata, 7, getcell(indata, 4));
    setcell(&outdata, 8, getcell(indata, 8));
    setcell(&outdata, 9, getcell(indata, 13));
    setcell(&outdata, 10, getcell(indata, 7));
    setcell(&outdata, 11, getcell(indata, 2));
    setcell(&outdata, 12, getcell(indata, 5));
    setcell(&outdata, 13, getcell(indata, 0));
    setcell(&outdata, 14, getcell(indata, 10));
    setcell(&outdata, 15, getcell(indata, 15));
    return outdata;
}

BIT64
mkw_qarma_PACCellShuffle(BIT64 indata)
{
    BIT64 outdata = 0x0;
    setcell(&outdata, 0, getcell(indata, 13));
    setcell(&outdata, 1, getcell(indata, 6));
    setcell(&outdata, 2, getcell(indata, 11));
    setcell(&outdata, 3, getcell(indata, 0));
    setcell(&outdata, 4, getcell(indata, 7));
    setcell(&outdata, 5, getcell(indata, 12));
    setcell(&outdata, 6, getcell(indata, 1));
    setcell(&outdata, 7, getcell(indata, 10));
    setcell(&outdata, 8, getcell(indata, 8));
    setcell(&outdata, 9, getcell(indata, 3));
    setcell(&outdata, 10, getcell(indata, 14));
    setcell(&outdata, 11, getcell(indata, 5));
    setcell(&outdata, 12, getcell(indata, 2));
    setcell(&outdata, 13, getcell(indata, 9));
    setcell(&outdata, 14, getcell(indata, 4));
    setcell(&outdata, 15, getcell(indata, 15));
    return outdata;
}


uint64_t
mkw_qarma_PACInvSub(uint64_t tInput)
{
    // This is a 4-bit substitution from the PRINCE-family cipher
    uint64_t t_output = 0x0;
    for (int i=15; i>=0; i--) {
        t_output = t_output << 4;
        uint8_t b = (tInput >> i*4 ) & 0xF;
        switch ( b ) {
            case 0x0:
                t_output |= 0x5;
                break;
            case 0x1:
                t_output |= 0xe;
                break;
            case 0x2:
                t_output |= 0xd;
                break;
            case 0x3:
                t_output |= 0x8;
                break;
            case 0x4:
                t_output |= 0xa;
                break;
            case 0x5:
                t_output |= 0xb;
                break;
            case 0x6:
                t_output |= 0x1;
                break;
            case 0x7:
                t_output |= 0x9;
                break;
            case 0x8:
                t_output |= 0x2;
                break;
            case 0x9:
                t_output |= 0x6;
                break;
            case 0xa:
                t_output |= 0xf;
                break;
            case 0xb:
                t_output |= 0x0;
                break;
            case 0xc:
                t_output |= 0x4;
                break;
            case 0xd:
                t_output |= 0xc;
                break;
            case 0xe:
                t_output |= 0x7;
                break;
            case 0xf:
                t_output |= 0x3;
                break;
            default:
                //unreachable
                break;
        }
    }
    return t_output;
}

uint64_t
mkw_qarma_PACSub(uint64_t tInput){
    // This is a 4-bit substitution from the PRINCE-family cipher
    uint64_t t_output = 0x0;
    for (int i=15; i>=0; i--) {
        t_output = t_output << 4;
        uint8_t b = (tInput >> i*4 ) & 0xF;
        switch ( b ) {
            case 0x0:
                t_output |= 0xb;
                break;
            case 0x1:
                t_output |= 0x6;
                break;
            case 0x2:
                t_output |= 0x8;
                break;
            case 0x3:
                t_output |= 0xf;
                break;
            case 0x4:
                t_output |= 0xc;
                break;
            case 0x5:
                t_output |= 0x0;
                break;
            case 0x6:
                t_output |= 0x9;
                break;
            case 0x7:
                t_output |= 0xe;
                break;
            case 0x8:
                t_output |= 0x3;
                break;
            case 0x9:
                t_output |= 0x7;
                break;
            case 0xa:
                t_output |= 0x4;
                break;
            case 0xb:
                t_output |= 0x5;
                break;
            case 0xc:
                t_output |= 0xd;
                break;
            case 0xd:
                t_output |= 0x2;
                break;
            case 0xe:
                t_output |= 0x1;
                break;
            case 0xf:
                t_output |= 0xa;
                break;
            default:
                //unreachable
                break;
        }
    }
    return t_output;
}

uint64_t
mkw_qarma_PACMult(uint64_t tInput)
{
    uint64_t t_output = 0;

    for (int i=0;i<=3; i++) {
        uint8_t b8  = (tInput >> (4*(i+8)))  & 0xF;
        uint8_t b4  = (tInput >> (4*(i+4)))  & 0xF;
        uint8_t b12 = (tInput >> (4*(i+12))) & 0xF;
        uint8_t b0  = (tInput >> (4*(i)))    & 0xF;

        uint64_t t0 = mkw_qarma_rotCell(b8, 1) ^ mkw_qarma_rotCell(b4, 2);
        t0 = t0 ^ mkw_qarma_rotCell(b0, 1);

        uint64_t t1 = mkw_qarma_rotCell(b12, 1) ^ mkw_qarma_rotCell(b4, 1);
        t1 = t1 ^ mkw_qarma_rotCell(b0, 2);

        uint64_t t2 = mkw_qarma_rotCell(b12, 2) ^ mkw_qarma_rotCell(b8, 1);
        t2 = t2 ^ mkw_qarma_rotCell(b0, 1);

        uint64_t t3 = mkw_qarma_rotCell(b12, 1) ^ mkw_qarma_rotCell(b8, 2);
        t3 = t3 ^ mkw_qarma_rotCell(b4, 1);

        t_output |= (t3 << (4*i));
        t_output |= (t2 << (4*(i+4)));
        t_output |= (t1 << (4*(i+8)));
        t_output |= (t0 << (4*(i+12)));
    }
    return t_output;
}

BIT64
mkw_qarma_computePAC(BIT64 data, BIT64 modifier, BIT64 key0, BIT64 key1)
{
    BIT64 workingval;
    BIT64 runningmod;
    BIT64 roundkey;
    BIT64 modk0;
    BIT64 RC[5];
    RC[0] = (BIT64) 0x0000000000000000;
    RC[1] = (BIT64) 0x13198A2E03707344;
    RC[2] = (BIT64) 0xA4093822299F31D0;
    RC[3] = (BIT64) 0x082EFA98EC4E6C89;
    RC[4] = (BIT64) 0x452821E638D01377;

    const BIT64 alpha = 0xC0AC29B7C97C50DD;
    //modk0 = key0<0>:key0<63:2>:

    modk0 = (key0 & 0x1) << 63;
    modk0 = modk0 | ((key0 & ~0x3) >> 1);
    modk0 = modk0 | ((getcell(key0, 15)>>3) ^ ((getcell(key0, 0) & 0x2)>>1));

    runningmod = modifier;
    workingval = data^key0;
    for (int i=0; i<=4; i++) {
        roundkey = key1 ^ runningmod;
        workingval = workingval ^ roundkey;
        workingval = workingval ^ RC[i];

        if (i > 0) {
            workingval = mkw_qarma_PACCellShuffle(workingval);
            workingval = mkw_qarma_PACMult(workingval);
        }
        workingval = mkw_qarma_PACSub(workingval);
        runningmod = mkw_qarma_tweakShuffle(runningmod);
    }
    roundkey = modk0 ^ runningmod;
    workingval = workingval ^ roundkey;

    workingval = mkw_qarma_PACCellShuffle(workingval);
    workingval = mkw_qarma_PACMult(workingval);
    workingval = mkw_qarma_PACSub(workingval);
    workingval = mkw_qarma_PACCellShuffle(workingval);
    workingval = mkw_qarma_PACMult(workingval);
    workingval = key1 ^ workingval;

    workingval = mkw_qarma_PACCellInvShuffle(workingval);
    workingval = mkw_qarma_PACInvSub(workingval);
    workingval = mkw_qarma_PACMult(workingval);
    workingval = mkw_qarma_PACCellInvShuffle(workingval);
    workingval = workingval ^ key0;
    workingval = workingval ^ runningmod;

    for (int i=0; i<=4; i++) {
        workingval = mkw_qarma_PACInvSub(workingval);
        if (i < 4) {
            workingval = mkw_qarma_PACMult(workingval);
            workingval = mkw_qarma_PACCellInvShuffle(workingval);
        }
        runningmod = mkw_qarma_tweakInvShuffle(runningmod);
        roundkey = key1 ^ runningmod;
        workingval = workingval ^ RC[4-i];
        workingval = workingval ^ roundkey;
        workingval = workingval ^ alpha;
    }
    workingval = workingval ^ modk0;
    return workingval;
}


