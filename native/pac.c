// -*- mode:c++ -*-

// Copyright (c) 2020 ARM Limited
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

// Adapted legacy PAC insertion/authentication for an explicit diagnostic profile.
#include "pac.h"
#include "../third_party/gem5-qarma/qarma.h"
#include <stdbool.h>
uint64_t mkw_qarma5(uint64_t data,uint64_t modifier,uint64_t high,uint64_t low) {
    return mkw_qarma_computePAC(data,modifier,high,low);
}
int mkw_pac39(void *context,uint32_t operation,uint32_t key,uint64_t pointer,uint64_t modifier,uint64_t *result) {
    if (!context || !result || key>1 || operation>3) return 3;
    const MkwPac39 *state=(const MkwPac39*)context;
    const uint64_t lowmask=(UINT64_C(1)<<39)-1;
    const uint64_t pacmask=~(lowmask|(UINT64_C(1)<<55));
    /* Sign uses bit 63 to choose extension; auth/strip recover it from bit 55. */
    const bool upper=((pointer>>(operation==0?63:55))&1)!=0;
    const uint64_t original=(pointer&lowmask)|(upper?~lowmask:0);
    if(operation==2){*result=original;return 0;}
    uint64_t pac=mkw_qarma5(original,modifier,state->high[key],state->low[key]);
    if(operation==0){
        const uint64_t extension=pointer&~lowmask;
        if(extension!=0 && extension!=~lowmask)pac^=UINT64_C(1)<<62;
        *result=(pointer&lowmask)|(((uint64_t)upper)<<55)|(pac&pacmask);return 0;
    }
    if((pointer&pacmask)==(pac&pacmask)){*result=original;return 0;}
    /* Legacy AUT poisons bits 62:61; authenticated return faults before returning. */
    *result=(original&~(UINT64_C(3)<<61))|(((uint64_t)(key+1))<<61);
    return operation==3?4:0;
}
