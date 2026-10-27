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

#ifndef MKW_QARMA_H
#define MKW_QARMA_H
#include <stdint.h>
typedef uint64_t BIT64;
uint8_t mkw_qarma_rotCell(uint8_t,int);
uint8_t mkw_qarma_tweakCellInvRot(uint8_t);
uint8_t mkw_qarma_tweakCellRot(uint8_t);
BIT64 mkw_qarma_tweakInvShuffle(BIT64);
BIT64 mkw_qarma_tweakShuffle(BIT64);
BIT64 mkw_qarma_PACCellInvShuffle(BIT64);
BIT64 mkw_qarma_PACCellShuffle(BIT64);
BIT64 mkw_qarma_PACInvSub(BIT64);
BIT64 mkw_qarma_PACSub(BIT64);
BIT64 mkw_qarma_PACMult(BIT64);
BIT64 mkw_qarma_computePAC(BIT64,BIT64,BIT64,BIT64);
#endif
