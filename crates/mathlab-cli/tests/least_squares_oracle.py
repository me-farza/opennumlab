#!/usr/bin/env python3
"""Independent complete-word/Fraction certificate for the fixed noisy LS family.

No QR factors, pivot ordering or Rust outputs generate expectations. Exported R is
A*X-B (the script convention); mathematical residual norms are sign invariant.
Only this diagonal-block family requires one nearest-even residual rounding;
there is no general correct-rounding or stability claim. Zero signs are free.
"""
import argparse
from fractions import Fraction as F
import hashlib
import json
import math
import os
from pathlib import Path
import re
import stat
import struct
import sys

sys.dont_write_bytecode = True
M, N, RHS = 4096, 32, 8
SECTIONS = (131072, 32768, 256, 32768)
WORDS = sum(SECTIONS)
PAYLOAD_BYTES = WORDS * 16
META_LIMIT, OUTCOME_LIMIT, SOURCE_LIMIT = 32768, 1 << 20, 65536
BINDING = 'ls_validation'


class OracleFailure(ValueError):
    pass


def require(condition, message):
    if not condition:
        raise OracleFailure(message)


def exact(actual, expected, label):
    require(type(actual) is type(expected), label + ': type')
    if isinstance(expected, dict):
        require(actual.keys() == expected.keys(), label + ': fields')
        for key in expected:
            exact(actual[key], expected[key], label + '.' + key)
    elif isinstance(expected, list):
        require(len(actual) == len(expected), label + ': length')
        for left, right in zip(actual, expected):
            exact(left, right, label)
    else:
        require(actual == expected, label + ': value')


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, 'duplicate JSON field')
        result[key] = value
    return result


def reject_constant(_):
    raise OracleFailure('nonstandard JSON constant')


def parse_json(raw, label):
    try:
        value = json.loads(raw.decode('utf-8'), object_pairs_hook=unique_object,
                           parse_constant=reject_constant)
    except (UnicodeError, json.JSONDecodeError) as error:
        raise OracleFailure(label + ': invalid JSON') from error
    require(type(value) is dict, label + ': object')
    return value


def read_bounded(path, limit, label):
    # Like the admitted loader, regular symlink targets are allowed; descriptor
    # checks prevent opening a FIFO/device as a blocking test input.
    fd = os.open(str(path), os.O_RDONLY | os.O_NONBLOCK | os.O_CLOEXEC)
    try:
        before = os.fstat(fd)
        require(stat.S_ISREG(before.st_mode) and before.st_size <= limit,
                label + ': regular bounded file')
        chunks, size = [], 0
        while True:
            block = os.read(fd, min(65536, limit + 1 - size))
            if not block:
                break
            chunks.append(block)
            size += len(block)
            require(size <= limit, label + ': read bound')
        after = os.fstat(fd)
        require((before.st_dev, before.st_ino, before.st_size, before.st_mtime_ns)
                == (after.st_dev, after.st_ino, after.st_size, after.st_mtime_ns),
                label + ': changed during read')
        return b''.join(chunks)
    finally:
        os.close(fd)


def graph_sha(source):
    path = source['path_display'].encode('utf-8')
    return hashlib.sha256(struct.pack('<Q', len(path)) + path
                          + struct.pack('<Q', source['id'])
                          + struct.pack('<Q', source['bytes'])
                          + bytes.fromhex(source['sha256'])).hexdigest()


def uints(value, keys, label):
    require(type(value) is dict and set(value) == set(keys), label + ': fields')
    require(all(type(value[k]) is int and 0 <= value[k] < (1 << 64)
                for k in keys), label + ': unsigned integers')
    return value


def check_outcome(raw, source_path, source_raw, expected_source_sha=None):
    require(len(raw) <= OUTCOME_LIMIT, 'outcome bound')
    lines = raw.splitlines(keepends=True)
    require(len(lines) == 2 and all(x.endswith(b'\n') for x in lines),
            'exactly two LF-terminated ready/outcome rows')
    rows = [parse_json(x[:-1], 'outcome row') for x in lines]
    digest = hashlib.sha256(source_raw).hexdigest()
    if expected_source_sha is not None:
        require(re.fullmatch(r'[0-9a-fA-F]{64}', expected_source_sha) is not None,
                'expected source hash format')
        require(digest == expected_source_sha.lower(), 'expected source hash')
    descriptor = None
    for row, event in zip(rows, ['ready', 'outcome']):
        for field, value in [('schema_version', 4), ('kind', 'mathlab-rust-program'),
                             ('backend', 'rust'), ('profile', 'class-v1'),
                             ('event', event), ('source_graph_complete', True)]:
            require(field in row, 'missing record.' + field)
            exact(row[field], value, 'record.' + field)
        source = row.get('source')
        require(type(source) is dict, 'source descriptor')
        display = source.get('path_display')
        require(type(display) is str and bool(display), 'source path')
        require(Path(display).resolve() == Path(source_path).resolve(),
                'source path identity')
        exact(source, dict(id=1, path_display=display, sha256=digest,
                           bytes=len(source_raw)), 'source')
        exact(row.get('source_graph'), [source], 'single-source closure')
        exact(row.get('source_graph_sha256'), graph_sha(source), 'closure hash')
        if descriptor is not None:
            exact(source, descriptor, 'ready/outcome source')
        descriptor = source
    ready, result = rows
    key = uints(ready.get('key'), ['session_id', 'epoch', 'request_id'], 'key')
    require(key['session_id'] > 0 and key['request_id'] > 0, 'active request key')
    exact(result.get('key'), key, 'ready/outcome request key')
    before = uints(ready.get('context'), ['session_id', 'epoch', 'revision'], 'context')
    require((before['session_id'], before['epoch']) == (key['session_id'], key['epoch']),
            'request/context owner')
    exact(result.get('before'), before, 'outcome before context')
    after = uints(result.get('after'), ['session_id', 'epoch', 'revision'], 'after')
    require((after['session_id'], after['epoch']) == (key['session_id'], key['epoch'])
            and after['revision'] >= before['revision'], 'outcome context continuity')
    for field, value in [('status', 'success'), ('phase', 'execution'),
                         ('command_exit', 0), ('events_complete', True),
                         ('reason', None), ('error', None)]:
        require(field in result, 'missing outcome.' + field)
        exact(result[field], value, 'outcome.' + field)
    workspace = result.get('workspace')
    require(type(workspace) is list and all(type(x) is dict for x in workspace),
            'workspace list')
    names = [x.get('name') for x in workspace]
    require(all(type(x) is str for x in names) and len(names) == len(set(names)),
            'unique workspace bindings')
    export = result.get('export')
    require(type(export) is dict, 'export result')
    for field, value in [('binding', BINDING), ('status', 'complete'),
                         ('complete', True), ('code', None)]:
        require(field in export, 'missing export.' + field)
        exact(export[field], value, 'export.' + field)
    exact(export.get('payload_bytes'), PAYLOAD_BYTES, 'export byte count')
    require(type(export.get('sha256')) is str
            and re.fullmatch(r'[0-9a-f]{64}', export['sha256']) is not None,
            'export hash')
    return rows, descriptor, export


def check_export(raw, source_sha, export):
    require(len(raw) <= PAYLOAD_BYTES + 2 * META_LIMIT + 1, 'export file bound')
    split = raw.find(b'\n')
    require(0 <= split < META_LIMIT, 'bounded LF header')
    exact(parse_json(raw[:split], 'header'), dict(
        format='onl-scientific-array-v1', **{'class': 'double'}, complex=True,
        shape=[WORDS, 1], elements=WORDS,
        component_encoding='ieee754-binary64-le',
        storage='column-major-interleaved', binding=BINDING,
        source_sha256=source_sha), 'dense header')
    start, end = split + 1, split + 1 + PAYLOAD_BYTES
    require(raw[end:end + 1] == b'\n', 'complete payload separator')
    tail = raw[end + 1:]
    require(0 < len(tail) <= META_LIMIT and tail.endswith(b'\n')
            and tail.count(b'\n') == 1, 'one bounded footer, no trailing junk')
    payload = raw[start:end]
    require(len(payload) == PAYLOAD_BYTES, 'complete payload')
    digest = hashlib.sha256(payload).hexdigest()
    exact(parse_json(tail[:-1], 'footer'), dict(complete=True,
          payload_bytes=PAYLOAD_BYTES, sha256=digest), 'footer')
    exact(export['sha256'], digest, 'outcome/payload digest')
    return payload


def check_workspace(rows, payload):
    entries = {x['name']: x.get('value') for x in rows[1]['workspace']}
    offset = 0
    for name, shape, count in [('ls_A', [M, N], SECTIONS[0]),
                               ('ls_B', [M, RHS], SECTIONS[1]),
                               ('ls_X', [N, RHS], SECTIONS[2]),
                               ('ls_R', [M, RHS], SECTIONS[3]),
                               (BINDING, [WORDS, 1], WORDS)]:
        value = entries.get(name)
        require(type(value) is dict, name + ': typed binding')
        window = min(256, count)
        expected = {'class': 'double', 'complex': True, 'storage': 'dense',
                    'storage_order': 'column-major', 'rank': 2, 'shape': shape,
                    'logical_numel': count, 'window_offset': 0,
                    'window_count': window, 'payload_complete': window == count}
        for field, data in expected.items():
            require(field in value, name + ': missing ' + field)
            exact(value[field], data, name + '.' + field)
        components = value.get('binary64_components')
        require(type(components) is list and len(components) == window,
                name + ': preview count')
        base = 0 if name == BINDING else offset
        for i, pair in enumerate(components):
            require(type(pair) is list and len(pair) == 2 and all(
                type(x) is str and re.fullmatch(r'[0-9a-f]{16}', x) for x in pair),
                name + ': preview word encoding')
            require(tuple(int(x, 16) for x in pair)
                    == struct.unpack_from('<QQ', payload, 16 * (base + i)),
                    name + ': preview/export word mismatch')
        if name != BINDING:
            offset += count


def rounded_word(q):
    """Exact rational -> nearest-even binary64 bits, including normal/subnormal carry."""
    q = F(q)
    if not q:
        return 0
    sign = (1 << 63) if q < 0 else 0
    a, b = abs(q.numerator), q.denominator
    exponent = a.bit_length() - b.bit_length()
    if (a < (b << exponent)) if exponent >= 0 else ((a << -exponent) < b):
        exponent -= 1
    shift = 1074 if exponent < -1022 else 52 - exponent
    numerator, denominator = (a << shift, b) if shift >= 0 else (a, b << -shift)
    integer, remainder = divmod(numerator, denominator)
    if 2 * remainder > denominator or (2 * remainder == denominator and integer & 1):
        integer += 1
    if exponent < -1022:
        return sign | integer
    if integer == (1 << 53):
        integer >>= 1
        exponent += 1
    if exponent > 1023:
        return sign | (0x7ff << 52)
    return sign | ((exponent + 1023) << 52) | (integer - (1 << 52))


def word_matches(bits, q):
    # Zero signs are intentionally not normalized in the retained payload.
    return (bits & ((1 << 63) - 1)) == 0 if not q else bits == rounded_word(q)


def coefficient(row):
    return [(F(1), F(0)), (F(0), F(2)),
            (F(-4), F(0)), (F(0), F(-8))][row // 8]


def apply_c(z, row, adjoint=False):
    re_part, im_part = z
    group = row // 8
    if group == 0:
        return re_part, im_part
    if group == 1:
        return (2 * im_part, -2 * re_part) if adjoint else (-2 * im_part, 2 * re_part)
    if group == 2:
        return -4 * re_part, -4 * im_part
    return (-8 * im_part, 8 * re_part) if adjoint else (8 * im_part, -8 * re_part)


def xstar(row, rhs):
    row, rhs = row + 1, rhs + 1
    return F(row + 2 * rhs, 64), F(2 * row - rhs, 128)


def family_b(block, row, rhs):
    weight = block + 1 if block < 127 else -8128
    ax = apply_c(xstar(row, rhs), row)
    return ax[0] + F(weight, 16), ax[1] + F(weight * (rhs + 1), 256)


def norm2(z):
    return z[0] * z[0] + z[1] * z[1]


def payload_pair(payload, index):
    return struct.unpack_from('<QQ', payload, 16 * index)


def require_exact_pair(payload, index, expected, label):
    bits = payload_pair(payload, index)
    require(all(word_matches(b, q) for b, q in zip(bits, expected)), label)


def check_payload(payload):
    require(len(payload) == PAYLOAD_BYTES, 'mathematical payload length')
    a_words, b_words, x_words, _ = SECTIONS
    b_start, x_start, r_start = a_words, a_words + b_words, a_words + b_words + x_words
    # Every matrix word is independently fixed by row/column geometry, including
    # the 126976 structural zeros. Never trust the exported A as the oracle.
    for col in range(N):
        for row in range(M):
            expected = coefficient(col) if row % N == col else (F(0), F(0))
            require_exact_pair(payload, col * M + row, expected, 'A family word')
    for rhs in range(RHS):
        for block in range(128):
            for row in range(N):
                require_exact_pair(payload, b_start + rhs * M + block * N + row,
                                   family_b(block, row, rhs), 'B family word')
    solution, error, x_norm2, e_norm2 = [], [], F(0), F(0)
    for rhs in range(RHS):
        for row in range(N):
            pair = struct.unpack_from('<dd', payload, 16 * (x_start + rhs * N + row))
            require(all(math.isfinite(x) for x in pair), 'nonfinite solution')
            z = tuple(F.from_float(x) for x in pair)
            reference = xstar(row, rhs)
            e = z[0] - reference[0], z[1] - reference[1]
            solution.append(z)
            error.append(e)
            x_norm2 += norm2(reference)
            e_norm2 += norm2(e)
    require(x_norm2 == F(8369, 128), 'frozen X family identity')
    forward_squared = e_norm2 / x_norm2
    require(forward_squared <= F(1, 1 << 30), 'relative forward bound')
    s = [[F(0), F(0)] for _ in solution]
    residual_squared = F(0)
    for rhs in range(RHS):
        for block in range(128):
            for row in range(N):
                index = rhs * N + row
                ax = apply_c(solution[index], row)
                b = family_b(block, row, rhs)
                true_residual = ax[0] - b[0], ax[1] - b[1]  # script R=AX-B
                # Check export bits against ONE exact nearest-even rounding;
                # metrics below use true rationals, not rounded residual words.
                require_exact_pair(payload, r_start + rhs * M + block * N + row,
                                   true_residual, 'rounded true residual word')
                residual_squared += norm2(true_residual)
                contribution = apply_c(true_residual, row, adjoint=True)
                s[index][0] += contribution[0]
                s[index][1] += contribution[1]
    weights_squared = sum(w * w for w in list(range(1, 128)) + [-8128])
    optimum = F(weights_squared, 256) * F(563, 2)
    require(weights_squared == 66755264 and optimum == F(587237713, 8),
            'frozen residual family identity')
    stationarity_squared = sum(norm2(z) for z in s) / (8192 * 8192 * x_norm2)
    weighted_error = F(0)
    for index, e in enumerate(error):
        row = index % N
        diagonal = 128 * (1, 4, 16, 64)[row // 8]
        require(tuple(s[index]) == (diagonal * e[0], diagonal * e[1]),
                'exact global stationarity identity')
        weighted_error += diagonal * norm2(e)
    excess = residual_squared - optimum
    require(excess == weighted_error and excess >= 0, 'exact objective identity')
    objective_relative = excess / optimum
    require(stationarity_squared <= F(1, 1 << 42), 'normalized stationarity bound')
    require(objective_relative <= F(1, 1 << 37), 'relative objective excess bound')
    return dict(complex_words_checked=WORDS, binary64_components_checked=WORDS * 2,
                sections=list(SECTIONS), relative_forward_squared=str(forward_squared),
                normalized_stationarity_squared=str(stationarity_squared),
                relative_objective_excess=str(objective_relative),
                true_residual_frobenius_squared=str(residual_squared),
                optimum_residual_frobenius_squared=str(optimum),
                zero_sign_qualification='free; raw payload retained unchanged',
                residual_export_sign='A*X-B',
                bounds={'relative_forward': '2^-15', 'normalized_stationarity': '2^-21',
                        'relative_objective_excess': '2^-37'})


def certify(source_path, outcome_path, export_path, expected_source_sha=None):
    source_raw = read_bounded(source_path, SOURCE_LIMIT, 'source')
    source_raw.decode('utf-8')
    outcome_raw = read_bounded(outcome_path, OUTCOME_LIMIT, 'outcome')
    rows, descriptor, export = check_outcome(outcome_raw, source_path, source_raw,
                                             expected_source_sha)
    raw = read_bounded(export_path, PAYLOAD_BYTES + 2 * META_LIMIT + 1, 'export')
    payload = check_export(raw, descriptor['sha256'], export)
    check_workspace(rows, payload)
    metrics = check_payload(payload)
    require(read_bounded(source_path, SOURCE_LIMIT, 'source') == source_raw,
            'source changed during certificate')
    return dict(status='passed', source_sha256=descriptor['sha256'],
                source_graph_sha256=graph_sha(descriptor), **metrics)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ['source', 'outcome', 'export']:
        parser.add_argument('--' + name, required=True)
    parser.add_argument('--expected-source-sha256',
                        help='compile-time source pin; required by the production CLI fixture')
    args = parser.parse_args()
    try:
        result = certify(args.source, args.outcome, args.export,
                         args.expected_source_sha256)
    except (OracleFailure, OSError, UnicodeError, OverflowError, ValueError) as error:
        print(json.dumps(dict(status='failed', error=str(error)), sort_keys=True))
        return 1
    print(json.dumps(result, sort_keys=True))
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
