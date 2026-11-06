#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Independent exact-rational oracle for the COMPLETE typed CLI export.
No numerical library, kernel implementation or Rust-computed oracle values.
Usage: python3 scientific_oracle.py full-Y.onlsci
"""
from fractions import Fraction as F
import hashlib,json,math,struct,sys
MAX_PAYLOAD=16*1024*1024

def cadd(a,b):return a[0]+b[0],a[1]+b[1]
def csub(a,b):return a[0]-b[0],a[1]-b[1]
def cmul(a,b):return a[0]*b[0]-a[1]*b[1],a[0]*b[1]+a[1]*b[0]
def norm(z):return abs(z[0])+abs(z[1])
def xstar(r,c,omega):return F(r+c),F(r-c)+omega
def rhs(r,c,omega):
    z=cmul((F(4),omega),xstar(r,c,omega))
    if r>1:z=csub(z,xstar(r-1,c,omega))
    if r<128:z=csub(z,xstar(r+1,c,omega))
    return z

def load(path):
    with open(path,'rb') as stream:
        raw=stream.readline(32769)
        if not raw.endswith(b'\n') or len(raw)>32768:raise ValueError('header bound')
        header=json.loads(raw)
        expected={'format':'onl-scientific-array-v1','class':'double','complex':True,'shape':[128,32,8],'elements':32768,'component_encoding':'ieee754-binary64-le','storage':'column-major-interleaved'}
        for key,value in expected.items():
            if header.get(key)!=value:raise ValueError('unexpected '+key)
        size=32768*16
        payload=stream.read(size)
        if len(payload)!=size:raise ValueError('incomplete payload')
        separator=stream.read(1)
        if separator!=b'\n':raise ValueError('missing footer separator')
        raw_footer=stream.readline(32769)
        if not raw_footer.endswith(b'\n') or len(raw_footer)>32768:raise ValueError('footer bound')
        footer=json.loads(raw_footer)
        if footer.get('complete') is not True or footer.get('payload_bytes')!=size or footer.get('sha256')!=hashlib.sha256(payload).hexdigest():raise ValueError('footer integrity')
        if stream.read(1):raise ValueError('trailing bytes')
    words=struct.unpack('<'+('d'*65536),payload)
    if not all(math.isfinite(x) for x in words):raise ValueError('nonfinite returned word')
    return [(F.from_float(words[i]),F.from_float(words[i+1])) for i in range(0,len(words),2)]

def verify(values):
    worst_eta=F(0);worst_forward=F(0)
    for page in range(8):
        omega=F(page+1,8)
        def got(r,c):return values[(r-1)+128*(c-1)+128*32*page]
        residual=F(0);xn=F(0);bn=F(0);error=F(0);exactn=F(0)
        for r in range(1,129):
            rr=F(0);xx=F(0);bb=F(0);ee=F(0);ss=F(0)
            for c in range(1,33):
                y=got(r,c);ax=cmul((F(4),omega),y)
                if r>1:ax=csub(ax,got(r-1,c))
                if r<128:ax=csub(ax,got(r+1,c))
                b=rhs(r,c,omega);star=xstar(r,c,omega)
                rr+=norm(csub(b,ax));xx+=norm(y);bb+=norm(b);ee+=norm(csub(y,star));ss+=norm(star)
            residual=max(residual,rr);xn=max(xn,xx);bn=max(bn,bb);error=max(error,ee);exactn=max(exactn,ss)
        an=6+omega
        eta=residual/(an*xn+bn);forward=error/exactn
        if eta>F(1,2**40):raise AssertionError(('backward',page,eta))
        if forward>F(1,2**36):raise AssertionError(('forward',page,forward))
        # Separate exact rational condition/forward implication; same complex
        # component-L1 rectangular matrix infinity norm throughout.
        inv=(4+omega)/(16+omega*omega)
        if inv>F(5,17) or inv*2>=1:raise AssertionError('condition lemma')
        kappa=an*inv/(1-2*inv)
        if kappa>5:raise AssertionError('condition bound')
        implied=2*kappa*eta/(1-kappa*eta)
        if implied>=F(1,2**36):raise AssertionError('forward implication')
        worst_eta=max(worst_eta,eta);worst_forward=max(worst_forward,forward)
    return {'pages':8,'rhs_per_page':32,'complex_words':32768,'eta_numerator':str(worst_eta.numerator),'eta_denominator':str(worst_eta.denominator),'forward_numerator':str(worst_forward.numerator),'forward_denominator':str(worst_forward.denominator)}
# Frozen independent Root-reviewed input words/policies, never Rust outputs.
DIVISION_CASES=json.loads('[{"id":"D01","input":["7fefffffffffffff","0000000000000001","3ff0000000000000","0000000000000000"],"reference":["7fefffffffffffff","0000000000000001"],"mode":0},{"id":"D02","input":["7fefffffffffffff","0000000000000001","bff0000000000000","0000000000000000"],"reference":["ffefffffffffffff","8000000000000001"],"mode":0},{"id":"D03","input":["7fefffffffffffff","0000000000000001","0000000000000000","3ff0000000000000"],"reference":["0000000000000001","ffefffffffffffff"],"mode":0},{"id":"D04","input":["7fefffffffffffff","0000000000000001","0000000000000000","bff0000000000000"],"reference":["8000000000000001","7fefffffffffffff"],"mode":0},{"id":"D05","input":["0000000000000001","7fefffffffffffff","3ff0000000000000","0000000000000000"],"reference":["0000000000000001","7fefffffffffffff"],"mode":0},{"id":"D06","input":["0000000000000001","7fefffffffffffff","bff0000000000000","0000000000000000"],"reference":["8000000000000001","ffefffffffffffff"],"mode":0},{"id":"D07","input":["0000000000000001","7fefffffffffffff","0000000000000000","3ff0000000000000"],"reference":["7fefffffffffffff","8000000000000001"],"mode":0},{"id":"D08","input":["0000000000000001","7fefffffffffffff","0000000000000000","bff0000000000000"],"reference":["ffefffffffffffff","0000000000000001"],"mode":0},{"id":"D09","input":["ffefffffffffffff","0000000000000001","3ff0000000000000","0000000000000000"],"reference":["ffefffffffffffff","0000000000000001"],"mode":0},{"id":"D10","input":["ffefffffffffffff","0000000000000001","bff0000000000000","0000000000000000"],"reference":["7fefffffffffffff","8000000000000001"],"mode":0},{"id":"D11","input":["ffefffffffffffff","0000000000000001","0000000000000000","3ff0000000000000"],"reference":["0000000000000001","7fefffffffffffff"],"mode":0},{"id":"D12","input":["ffefffffffffffff","0000000000000001","0000000000000000","bff0000000000000"],"reference":["8000000000000001","ffefffffffffffff"],"mode":0},{"id":"D13","input":["0000000000000001","ffefffffffffffff","3ff0000000000000","0000000000000000"],"reference":["0000000000000001","ffefffffffffffff"],"mode":0},{"id":"D14","input":["0000000000000001","ffefffffffffffff","bff0000000000000","0000000000000000"],"reference":["8000000000000001","7fefffffffffffff"],"mode":0},{"id":"D15","input":["0000000000000001","ffefffffffffffff","0000000000000000","3ff0000000000000"],"reference":["ffefffffffffffff","8000000000000001"],"mode":0},{"id":"D16","input":["0000000000000001","ffefffffffffffff","0000000000000000","bff0000000000000"],"reference":["7fefffffffffffff","0000000000000001"],"mode":0},{"id":"D17","input":["7fb0000000000000","0030000000000000","4000000000000000","0000000000000000"],"reference":["7fa0000000000000","0020000000000000"],"mode":0},{"id":"D18","input":["7fb0000000000000","0030000000000000","c000000000000000","0000000000000000"],"reference":["ffa0000000000000","8020000000000000"],"mode":0},{"id":"D19","input":["7fb0000000000000","0030000000000000","3fe0000000000000","0000000000000000"],"reference":["7fc0000000000000","0040000000000000"],"mode":0},{"id":"D20","input":["7fb0000000000000","0030000000000000","bfe0000000000000","0000000000000000"],"reference":["ffc0000000000000","8040000000000000"],"mode":0},{"id":"D21","input":["0030000000000000","7fb0000000000000","4000000000000000","0000000000000000"],"reference":["0020000000000000","7fa0000000000000"],"mode":0},{"id":"D22","input":["0030000000000000","7fb0000000000000","c000000000000000","0000000000000000"],"reference":["8020000000000000","ffa0000000000000"],"mode":0},{"id":"D23","input":["0030000000000000","7fb0000000000000","3fe0000000000000","0000000000000000"],"reference":["0040000000000000","7fc0000000000000"],"mode":0},{"id":"D24","input":["0030000000000000","7fb0000000000000","bfe0000000000000","0000000000000000"],"reference":["8040000000000000","ffc0000000000000"],"mode":0},{"id":"D25","input":["7fefffffffffffff","0000000000000001","5ff0000000000000","0000000000000000"],"reference":["5fefffffffffffff","0000000000000000"],"mode":0},{"id":"D26","input":["7fefffffffffffff","0000000000000001","dff0000000000000","0000000000000000"],"reference":["dfefffffffffffff","8000000000000000"],"mode":0},{"id":"D27","input":["0000000000000001","7fefffffffffffff","5ff0000000000000","0000000000000000"],"reference":["0000000000000000","5fefffffffffffff"],"mode":0},{"id":"D28","input":["0000000000000001","7fefffffffffffff","dff0000000000000","0000000000000000"],"reference":["8000000000000000","dfefffffffffffff"],"mode":0},{"id":"D29","input":["5f30000000000000","0000000000000001","1ff0000000000000","0000000000000000"],"reference":["7f30000000000000","1cd0000000000000"],"mode":0},{"id":"D30","input":["5f30000000000000","0000000000000001","9ff0000000000000","0000000000000000"],"reference":["ff30000000000000","9cd0000000000000"],"mode":0},{"id":"D31","input":["0000000000000001","5f30000000000000","1ff0000000000000","0000000000000000"],"reference":["1cd0000000000000","7f30000000000000"],"mode":0},{"id":"D32","input":["0000000000000001","5f30000000000000","9ff0000000000000","0000000000000000"],"reference":["9cd0000000000000","ff30000000000000"],"mode":0},{"id":"D33","input":["0000000000000001","0000000000000001","0000000000000001","0000000000000001"],"reference":["3ff0000000000000","0000000000000000"],"mode":0},{"id":"D34","input":["0000000000000001","8000000000000001","0000000000000001","0000000000000001"],"reference":["0000000000000000","bff0000000000000"],"mode":0},{"id":"D35","input":["0000000000000002","0000000000000001","0000000000000001","0000000000000000"],"reference":["4000000000000000","3ff0000000000000"],"mode":0},{"id":"D36","input":["0000000000000001","0000000000000002","0000000000000000","0000000000000001"],"reference":["4000000000000000","bff0000000000000"],"mode":0},{"id":"D37","input":["0000000000000001","0000000000000001","3ff0000000000000","3ff0000000000000"],"reference":["0000000000000001","0000000000000000"],"mode":0},{"id":"D38","input":["0000000000000001","8000000000000001","3ff0000000000000","3ff0000000000000"],"reference":["0000000000000000","8000000000000001"],"mode":0},{"id":"D39","input":["0000000000000001","0000000000000000","4000000000000000","0000000000000000"],"reference":["0000000000000000","0000000000000000"],"mode":0},{"id":"D40","input":["0000000000000003","0000000000000000","4000000000000000","0000000000000000"],"reference":["0000000000000002","0000000000000000"],"mode":0},{"id":"D41","input":["7fefffffffffffff","7fefffffffffffff","7fefffffffffffff","7fefffffffffffff"],"reference":["3ff0000000000000","0000000000000000"],"mode":0},{"id":"D42","input":["7fefffffffffffff","ffefffffffffffff","7fefffffffffffff","7fefffffffffffff"],"reference":["0000000000000000","bff0000000000000"],"mode":0},{"id":"D43","input":["3ff0000000000000","3ff0000000000000","7fefffffffffffff","7fefffffffffffff"],"reference":["0004000000000000","0000000000000000"],"mode":1},{"id":"D44","input":["0000000000000001","0000000000000001","7fefffffffffffff","7fefffffffffffff"],"reference":["0000000000000000","0000000000000000"],"mode":0},{"id":"D45","input":["7fefffffffffffff","0000000000000001","7fefffffffffffff","7fefffffffffffff"],"reference":["3fe0000000000000","bfe0000000000000"],"mode":1},{"id":"D46","input":["0000000000000001","7fefffffffffffff","7fefffffffffffff","ffefffffffffffff"],"reference":["bfe0000000000000","3fe0000000000000"],"mode":1},{"id":"D47","input":["3ff0000000000001","3ff0000000000000","3ff0000000000000","3ff0000000000001"],"reference":["3ff0000000000000","bcafffffffffffff"],"mode":1},{"id":"D48","input":["3ff0000000000000","3ff0000000000001","3ff0000000000001","3ff0000000000000"],"reference":["3ff0000000000000","3cafffffffffffff"],"mode":1},{"id":"D49","input":["3ff0000000000000","3ff0000000000000","3ff0000000000000","3ff0000000000001"],"reference":["3fefffffffffffff","bc9ffffffffffffe"],"mode":1},{"id":"D50","input":["3ff0000000000000","3ff0000000000000","3ff0000000000000","3fefffffffffffff"],"reference":["3ff0000000000000","3c90000000000001"],"mode":1},{"id":"D51","input":["7fefffffffffffff","7feffffffffffffe","7feffffffffffffe","7fefffffffffffff"],"reference":["3ff0000000000000","bca0000000000001"],"mode":1},{"id":"D52","input":["5f30000000000000","5f30000000000001","5f30000000000001","5f30000000000000"],"reference":["3ff0000000000000","3cafffffffffffff"],"mode":1},{"id":"D53","input":["7fefffffffffffff","0000000000000001","0000000000000001","0000000000000000"],"reference":["0000000000000000","3ff0000000000000"],"mode":2},{"id":"D54","input":["0000000000000001","7fefffffffffffff","0000000000000001","0000000000000000"],"reference":["3ff0000000000000","0000000000000000"],"mode":2},{"id":"D55","input":["7fefffffffffffff","7fefffffffffffff","0000000000000001","0000000000000001"],"reference":["0000000000000000","0000000000000000"],"mode":2},{"id":"D56","input":["7fefffffffffffff","ffefffffffffffff","0000000000000001","0000000000000001"],"reference":["0000000000000000","0000000000000000"],"mode":2},{"id":"D57","input":["7fefffffffffffff","0000000000000000","3fe0000000000000","0000000000000000"],"reference":["0000000000000000","0000000000000000"],"mode":2},{"id":"D58","input":["0000000000000000","7fefffffffffffff","0000000000000000","3fe0000000000000"],"reference":["0000000000000000","0000000000000000"],"mode":2},{"id":"D59","input":["8000000000000000","0000000000000000","3ff0000000000000","0000000000000000"],"reference":["0000000000000000","0000000000000000"],"mode":0},{"id":"D60","input":["0000000000000000","8000000000000000","bff0000000000000","0000000000000000"],"reference":["0000000000000000","0000000000000000"],"mode":0},{"id":"D61","input":["8000000000000000","8000000000000000","0000000000000000","3ff0000000000000"],"reference":["0000000000000000","0000000000000000"],"mode":0},{"id":"D62","input":["0000000000000000","0000000000000000","0000000000000000","bff0000000000000"],"reference":["0000000000000000","0000000000000000"],"mode":0},{"id":"D63","input":["000fffffffffffff","0010000000000000","4000000000000000","0000000000000000"],"reference":["0008000000000000","0008000000000000"],"mode":0},{"id":"D64","input":["0010000000000000","000fffffffffffff","3ff0000000000000","3ff0000000000000"],"reference":["0010000000000000","8000000000000000"],"mode":0}]')

def division_words(path):
    observed={}
    with open(path,'rt',encoding='ascii') as stream:
        for line in stream:
            parts=line.split()
            if len(parts) not in (2,3) or parts[0] in observed:raise ValueError('division row')
            observed[parts[0]]=parts[1:]
    if len(observed)!=64:raise ValueError('division count')
    def number(word):
        v=struct.unpack('>d',bytes.fromhex(word))[0]
        if not math.isfinite(v):raise ValueError('nonfinite successful quotient')
        return F.from_float(v)
    residuals=0
    for case in DIVISION_CASES:
        got=observed[case['id']]
        if case['mode']==2:
            if got!=['ArithmeticBreakdown']:raise AssertionError(case['id'])
            continue
        if len(got)!=2:raise ValueError('missing quotient')
        q=number(got[0]),number(got[1])
        if case['mode']==0:
            for word,reference in zip(got,case['reference']):
                if int(reference,16)&0x7fffffffffffffff==0:
                    if number(word)!=0:raise AssertionError((case['id'],'required numerical zero'))
                elif word!=reference:raise AssertionError((case['id'],'independent required word'))
        else:
            a=number(case['input'][0]),number(case['input'][1]);b=number(case['input'][2]),number(case['input'][3])
            eta=norm(csub(a,cmul(b,q)))/(norm(a)+norm(b)*norm(q))
            if eta>F(1,2**47):raise AssertionError((case['id'],'64u residual',eta))
            residuals+=1
    if residuals!=9:raise AssertionError('residualcasecount')
    return {'division_cases':64,'independent_residual_cases':residuals,'rust_algorithm_read':False}
SOLVER_CASES=json.loads('[{"id":"S01","n":2,"rhs":2,"a":[["0000000000000000","0000000000000000"],["4000000000000000","bff0000000000000"],["3ff0000000000000","3ff0000000000000"],["4008000000000000","3fe0000000000000"]],"b":[["3ff0000000000000","3fe0000000000000"],["4013800000000000","bfd8000000000000"],["3fe0000000000000","3fe0000000000000"],["bff8000000000000","4011000000000000"]],"x":[["3ff0000000000000","3fe0000000000000"],["3fe8000000000000","bfd0000000000000"],["c000000000000000","3ff0000000000000"],["3fe0000000000000","0000000000000000"]],"mode":0,"near_singular":false,"well_family":false,"kappa":{"numerator":"221","denominator":"20"}},{"id":"S02","n":2,"rhs":2,"a":[["4000000000000000","bff0000000000000"],["0000000000000000","0000000000000000"],["4008000000000000","3fe0000000000000"],["3ff0000000000000","3ff0000000000000"]],"b":[["4013800000000000","bfd8000000000000"],["3ff0000000000000","3fe0000000000000"],["bff8000000000000","4011000000000000"],["3fe0000000000000","3fe0000000000000"]],"x":[["3ff0000000000000","3fe0000000000000"],["3fe8000000000000","bfd0000000000000"],["c000000000000000","3ff0000000000000"],["3fe0000000000000","0000000000000000"]],"mode":0,"near_singular":false,"well_family":false,"kappa":{"numerator":"221","denominator":"20"}},{"id":"S03","n":2,"rhs":2,"a":[["3ff0000000000000","3ff0000000000000"],["4008000000000000","3fe0000000000000"],["0000000000000000","0000000000000000"],["4000000000000000","bff0000000000000"]],"b":[["3ff0000000000000","3fe0000000000000"],["4013800000000000","bfd8000000000000"],["3fe0000000000000","3fe0000000000000"],["bff8000000000000","4011000000000000"]],"x":[["3fe8000000000000","bfd0000000000000"],["3ff0000000000000","3fe0000000000000"],["3fe0000000000000","0000000000000000"],["c000000000000000","3ff0000000000000"]],"mode":0,"near_singular":false,"well_family":false,"kappa":{"numerator":"221","denominator":"20"}},{"id":"S04","n":4,"rhs":2,"a":[["4010000000000000","3fe0000000000000"],["bff0000000000000","0000000000000000"],["0000000000000000","0000000000000000"],["0000000000000000","0000000000000000"],["bff0000000000000","0000000000000000"],["4010000000000000","3fe0000000000000"],["bff0000000000000","0000000000000000"],["0000000000000000","0000000000000000"],["0000000000000000","0000000000000000"],["bff0000000000000","0000000000000000"],["4010000000000000","3fe0000000000000"],["bff0000000000000","0000000000000000"],["0000000000000000","0000000000000000"],["0000000000000000","0000000000000000"],["bff0000000000000","0000000000000000"],["4010000000000000","3fe0000000000000"]],"b":[["3ff4000000000000","3fc0000000000000"],["3ff7000000000000","3fe4000000000000"],["3ffe000000000000","3ff0000000000000"],["400e800000000000","3ffe000000000000"],["4000800000000000","bfc0000000000000"],["4000000000000000","3fe0000000000000"],["4003800000000000","3fec000000000000"],["4012800000000000","3ffa000000000000"]],"x":[["3fe0000000000000","0000000000000000"],["3fe8000000000000","3fc0000000000000"],["3ff0000000000000","3fd0000000000000"],["3ff4000000000000","3fd8000000000000"],["3fe8000000000000","bfc0000000000000"],["3ff0000000000000","0000000000000000"],["3ff4000000000000","3fc0000000000000"],["3ff8000000000000","3fd0000000000000"]],"mode":0,"near_singular":false,"well_family":true,"kappa":{"numerator":"7033","denominator":"2045"}},{"id":"S05","n":3,"rhs":2,"a":[["0000000000000000","0000000000000000"],["4000000000000000","0000000000000000"],["0000000000000000","0000000000000000"],["0000000000000000","0000000000000000"],["0000000000000000","0000000000000000"],["4010000000000000","3ff0000000000000"],["3ff0000000000000","3ff0000000000000"],["4008000000000000","bff0000000000000"],["4014000000000000","0000000000000000"]],"b":[["3fe4000000000000","3ffe000000000000"],["4017800000000000","3fec000000000000"],["4023c00000000000","4016800000000000"],["3ff4000000000000","4002000000000000"],["4020800000000000","bfd0000000000000"],["402d000000000000","4014000000000000"]],"x":[["3fe8000000000000","3fc0000000000000"],["3ff0000000000000","3fd8000000000000"],["3ff4000000000000","3fe4000000000000"],["3ff4000000000000","0000000000000000"],["3ff8000000000000","3fd0000000000000"],["3ffc000000000000","3fe0000000000000"]],"mode":0,"near_singular":false,"well_family":false,"kappa":{"numerator":"20","denominator":"1"}},{"id":"S06","n":4,"rhs":2,"a":[["6590000000000000","6560000000000000"],["e570000000000000","0000000000000000"],["0000000000000000","0000000000000000"],["0000000000000000","0000000000000000"],["e570000000000000","0000000000000000"],["6590000000000000","6560000000000000"],["e570000000000000","0000000000000000"],["0000000000000000","0000000000000000"],["0000000000000000","0000000000000000"],["e570000000000000","0000000000000000"],["6590000000000000","6560000000000000"],["e570000000000000","0000000000000000"],["0000000000000000","0000000000000000"],["0000000000000000","0000000000000000"],["e570000000000000","0000000000000000"],["6590000000000000","6560000000000000"]],"b":[["6574000000000000","6540000000000000"],["6577000000000000","6564000000000000"],["657e000000000000","6570000000000000"],["658e800000000000","657e000000000000"],["6580800000000000","e540000000000000"],["6580000000000000","6560000000000000"],["6583800000000000","656c000000000000"],["6592800000000000","657a000000000000"]],"x":[["3fe0000000000000","0000000000000000"],["3fe8000000000000","3fc0000000000000"],["3ff0000000000000","3fd0000000000000"],["3ff4000000000000","3fd8000000000000"],["3fe8000000000000","bfc0000000000000"],["3ff0000000000000","0000000000000000"],["3ff4000000000000","3fc0000000000000"],["3ff8000000000000","3fd0000000000000"]],"mode":0,"near_singular":false,"well_family":true,"kappa":{"numerator":"7033","denominator":"2045"}},{"id":"S07","n":4,"rhs":2,"a":[["1a90000000000000","1a60000000000000"],["9a70000000000000","0000000000000000"],["0000000000000000","0000000000000000"],["0000000000000000","0000000000000000"],["9a70000000000000","0000000000000000"],["1a90000000000000","1a60000000000000"],["9a70000000000000","0000000000000000"],["0000000000000000","0000000000000000"],["0000000000000000","0000000000000000"],["9a70000000000000","0000000000000000"],["1a90000000000000","1a60000000000000"],["9a70000000000000","0000000000000000"],["0000000000000000","0000000000000000"],["0000000000000000","0000000000000000"],["9a70000000000000","0000000000000000"],["1a90000000000000","1a60000000000000"]],"b":[["1a74000000000000","1a40000000000000"],["1a77000000000000","1a64000000000000"],["1a7e000000000000","1a70000000000000"],["1a8e800000000000","1a7e000000000000"],["1a80800000000000","9a40000000000000"],["1a80000000000000","1a60000000000000"],["1a83800000000000","1a6c000000000000"],["1a92800000000000","1a7a000000000000"]],"x":[["3fe0000000000000","0000000000000000"],["3fe8000000000000","3fc0000000000000"],["3ff0000000000000","3fd0000000000000"],["3ff4000000000000","3fd8000000000000"],["3fe8000000000000","bfc0000000000000"],["3ff0000000000000","0000000000000000"],["3ff4000000000000","3fc0000000000000"],["3ff8000000000000","3fd0000000000000"]],"mode":0,"near_singular":false,"well_family":true,"kappa":{"numerator":"7033","denominator":"2045"}},{"id":"S08","n":2,"rhs":2,"a":[["3ff0000000000000","3fd0000000000000"],["3ff0000000000000","3fd0000000000000"],["3ff0000000000000","3fd0000000000000"],["3ff0000000001000","3fd0000000000000"]],"b":[["3ffb000000000000","3fe6000000000000"],["3ffb000000000c00","3fe5fffffffff800"],["bffc000000000000","3fe4000000000000"],["bffbfffffffff800","3fe4000000000000"]],"x":[["3ff0000000000000","3fe0000000000000"],["3fe8000000000000","bfd0000000000000"],["c000000000000000","3ff0000000000000"],["3fe0000000000000","0000000000000000"]],"mode":0,"near_singular":true,"well_family":false,"kappa":{"numerator":"25689673666833959706689541","denominator":"4672924418048"}},{"id":"S09","n":2,"rhs":1,"a":[["3ff0000000000000","3ff0000000000000"],["4000000000000000","4000000000000000"],["4000000000000000","bff0000000000000"],["4010000000000000","c000000000000000"]],"b":[["bfd0000000000000","4004000000000000"],["bfe0000000000000","4014000000000000"]],"x":[],"mode":1,"near_singular":false,"well_family":false,"kappa":null},{"id":"S10","n":2,"rhs":1,"a":[["7ff8000000000001","0000000000000000"],["0000000000000000","0000000000000000"],["0000000000000000","0000000000000000"],["3ff0000000000000","0000000000000000"]],"b":[["3ff0000000000000","0000000000000000"],["4000000000000000","0000000000000000"]],"x":[],"mode":2,"near_singular":false,"well_family":false,"kappa":null},{"id":"S11","n":2,"rhs":1,"a":[["3ff0000000000000","0000000000000000"],["0000000000000000","fff0000000000000"],["0000000000000000","0000000000000000"],["3ff0000000000000","0000000000000000"]],"b":[["3ff0000000000000","0000000000000000"],["4000000000000000","0000000000000000"]],"x":[],"mode":2,"near_singular":false,"well_family":false,"kappa":null},{"id":"S12","n":2,"rhs":1,"a":[["3ff0000000000000","0000000000000000"],["0000000000000000","0000000000000000"],["0000000000000000","0000000000000000"],["3ff0000000000000","0000000000000000"]],"b":[["3ff0000000000000","0000000000000000"],["7ff0000000000000","0000000000000000"]],"x":[],"mode":2,"near_singular":false,"well_family":false,"kappa":null}]')

def solver_words(path):
    observed={}
    with open(path,'rt',encoding='ascii') as stream:
        for line in stream:
            row=line.split()
            if not row or row[0] in observed:raise ValueError('solverrow')
            observed[row[0]]=row[1:]
    if len(observed)!=12:raise ValueError('solvercount')
    def component(word):
        value=struct.unpack('>d',bytes.fromhex(word))[0]
        if not math.isfinite(value):raise ValueError('nonfinite quotient')
        return F.from_float(value)
    def vector(words):return [(component(re),component(im)) for re,im in words]
    def matrix_norm(v,n,cols):return max((sum(norm(v[r+c*n]) for c in range(cols)) for r in range(n)),default=F(0))
    count=0
    for case in SOLVER_CASES:
        row=observed[case['id']];mode=case['mode']
        if mode:
            if row!=['Singular' if mode==1 else 'NonFiniteInput']:raise AssertionError(case['id'])
            continue
        n=case['n'];cols=case['rhs']
        if len(row)!=2*n*cols:raise ValueError('solverwordcount')
        q=[(component(row[i]),component(row[i+1])) for i in range(0,len(row),2)]
        a=vector(case['a']);b=vector(case['b']);target=vector(case['x']);residual=[]
        for c in range(cols):
            for r in range(n):
                ax=(F(0),F(0))
                for k in range(n):ax=cadd(ax,cmul(a[r+k*n],q[k+c*n]))
                residual.append(csub(b[r+c*n],ax))
        rn=matrix_norm(residual,n,cols);an=matrix_norm(a,n,n);qn=matrix_norm(q,n,cols);bn=matrix_norm(b,n,cols);den=an*qn+bn
        eta=rn/den if den else F(0)
        if not den and rn:raise AssertionError('zero residual denominator')
        limit=F(64*n,2**53)
        if eta>limit:raise AssertionError((case['id'],'backward',eta))
        if not case['near_singular']:
            kappa=F(int(case['kappa']['numerator']),int(case['kappa']['denominator']));positive=1-kappa*limit
            if positive<=0:raise AssertionError('condition denominator')
            forward_limit=2*kappa*limit/positive;error=matrix_norm([csub(x,y) for x,y in zip(q,target)],n,cols);targetn=matrix_norm(target,n,cols)
            if not targetn:
                if error:raise AssertionError('zero target')
            elif error/targetn>forward_limit:raise AssertionError((case['id'],'conditioned forward'))
            if case['well_family'] and targetn and error/targetn>F(1,2**36):raise AssertionError((case['id'],'familyforward'))
        count+=1
    if count!=8:raise AssertionError('uniquecount')
    return {'solver_systems':12,'unique':8,'singular':1,'nonfinite_rejection':3,'near_singular_forward_guarantee':False}
if __name__=='__main__':
    if len(sys.argv)==3 and sys.argv[1]=='--division':print(json.dumps(division_words(sys.argv[2]),sort_keys=True))
    elif len(sys.argv)==3 and sys.argv[1]=='--solver':print(json.dumps(solver_words(sys.argv[2]),sort_keys=True))
    elif len(sys.argv)==2:print(json.dumps(verify(load(sys.argv[1])),sort_keys=True))
    else:raise SystemExit('usage: scientific_oracle.py COMPLETE_EXPORT | --division WORD_RECORDS | --solver WORD_RECORDS')
