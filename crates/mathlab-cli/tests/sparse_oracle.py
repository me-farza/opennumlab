#!/usr/bin/env python3
"""Wire/source checks for Poisson exports; exact math belongs to pinned core."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import re
import stat
import struct
import sys

sys.dont_write_bytecode = True
CORE_SHA256 = 'f09d6d07fff035a1bf3a4bd4f7e24cafe20f7f4c4d74e2c5f1118a25e61ac9a1'
N = 32768
NNZ = 223232
META_LIMIT = 32768
PAYLOAD_LIMIT = 16 << 20
OUTCOME_LIMIT = 1 << 20


class WireFailure(ValueError):
    pass


def require(condition, message):
    if not condition:
        raise WireFailure(message)


def exact(actual, expected, label):
    require(type(actual) is type(expected), label + ': type mismatch')
    if isinstance(expected, dict):
        require(actual.keys() == expected.keys(), label + ': field mismatch')
        for key in expected:
            exact(actual[key], expected[key], label + '.' + key)
    elif isinstance(expected, list):
        require(len(actual) == len(expected), label + ': length mismatch')
        for left, right in zip(actual, expected):
            exact(left, right, label)
    else:
        require(actual == expected, label + ': value mismatch')


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, 'duplicate JSON field')
        result[key] = value
    return result


def reject_constant(_):
    raise WireFailure('nonstandard JSON constant')


def parse_json(raw, label):
    try:
        value = json.loads(raw.decode('utf-8'), object_pairs_hook=unique_object,
                           parse_constant=reject_constant)
    except (UnicodeError, json.JSONDecodeError) as error:
        raise WireFailure(label + ': invalid JSON') from error
    require(type(value) is dict, label + ': object required')
    return value


def read_bounded(path, limit, label):
    path = Path(path)
    require(stat.S_ISREG(path.stat().st_mode), label + ': regular file required')
    require(path.stat().st_size <= limit, label + ': file bound')
    with path.open('rb') as stream:
        raw = stream.read(limit + 1)
    require(len(raw) <= limit, label + ': read bound')
    return raw


def load_core():
    path = Path(__file__).with_name('poisson_fraction_core.py')
    raw = read_bounded(path, 65536, 'core')
    require(hashlib.sha256(raw).hexdigest() == CORE_SHA256, 'core hash mismatch')
    spec = importlib.util.spec_from_file_location('pinned_poisson_fraction_core', path)
    require(spec is not None and spec.loader is not None, 'core import unavailable')
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    # Execute the exact hashed bytes, never an existing timestamp-based pyc.
    exec(compile(raw, str(path), 'exec'), module.__dict__)
    return module


def scalar(workspace, name):
    entries = [entry for entry in workspace if entry.get('name') == name]
    require(len(entries) == 1, name + ': unique workspace binding required')
    value = entries[0].get('value')
    require(type(value) is dict, name + ': typed value required')
    for field, expected in [('class', 'double'), ('complex', False),
                            ('rank', 2), ('shape', [1, 1]), ('logical_numel', 1),
                            ('storage', 'dense'),
                            ('storage_order', 'column-major'), ('window_offset', 0),
                            ('window_count', 1), ('payload_complete', True)]:
        exact(value.get(field), expected, name + '.' + field)
    words = value.get('binary64_components')
    require(type(words) is list and len(words) == 1
            and type(words[0]) is list and len(words[0]) == 1,
            name + ': one real component required')
    word = words[0][0]
    require(type(word) is str and re.fullmatch(r'[0-9a-f]{16}', word) is not None,
            name + ': exact binary64 word required')
    return struct.unpack('>d', bytes.fromhex(word))[0]


def outcome(path, source_path, source_sha, source_bytes, binding):
    raw = read_bounded(path, OUTCOME_LIMIT, 'outcome')
    lines = raw.splitlines(keepends=True)
    require(len(lines) == 2 and all(line.endswith(b'\n') for line in lines),
            'exactly two LF-terminated ready/outcome rows required')
    rows = [parse_json(line, 'outcome row') for line in lines]
    source = None
    for row, event in zip(rows, ['ready', 'outcome']):
        for field, expected in [('schema_version', 3), ('backend', 'rust'),
                                ('profile', 'scientific-v2'),
                                ('kind', 'mathlab-rust-program'), ('event', event)]:
            exact(row.get(field), expected, 'record.' + field)
        identity = row.get('source')
        require(type(identity) is dict, 'source descriptor required')
        display = identity.get('path_display')
        require(type(display) is str and bool(display), 'source path required')
        require(Path(display).resolve() == source_path.resolve(), 'source path mismatch')
        exact(identity, {'id': 1, 'path_display': display,
                         'sha256': source_sha, 'bytes': source_bytes}, 'source')
        if source is not None:
            exact(identity, source, 'ready/outcome source')
        source = identity
    result = rows[1]
    for field, expected in [('status', 'success'), ('command_exit', 0),
                            ('events_complete', True), ('phase', 'execution'),
                            ('reason', None), ('error', None)]:
        require(field in result, 'missing outcome.' + field)
        exact(result[field], expected, 'outcome.' + field)
    workspace = result.get('workspace')
    require(type(workspace) is list and all(type(e) is dict for e in workspace),
            'workspace list required')
    names = [entry.get('name') for entry in workspace]
    require(all(type(name) is str for name in names) and len(names) == len(set(names)),
            'unique workspace names required')
    flag = scalar(workspace, 'flag')
    iterations = scalar(workspace, 'iterations')
    require(flag == 0.0, 'PCG flag must be zero')
    require(0 < iterations <= 512 and iterations.is_integer(),
            'PCG iterations must be an integer in 1..512')
    export = result.get('export')
    require(type(export) is dict, 'export outcome required')
    require({'binding', 'status', 'complete', 'code', 'payload_bytes', 'sha256'} <= export.keys(),
            'complete export outcome fields required')
    for field, expected in [('binding', binding), ('status', 'complete'),
                            ('complete', True), ('code', None)]:
        exact(export.get(field), expected, 'export outcome.' + field)
    require(type(export.get('payload_bytes')) is int, 'export payload size required')
    require(type(export.get('sha256')) is str
            and re.fullmatch(r'[0-9a-f]{64}', export['sha256']) is not None,
            'export digest required')
    return source, export, int(iterations)


def export_file(path, expected_header, export_outcome, expected_bytes):
    require(0 <= expected_bytes <= PAYLOAD_LIMIT, 'payload bound')
    raw = read_bounded(path, PAYLOAD_LIMIT + 2 * META_LIMIT + 1, 'export')
    header_end = raw.find(b'\n')
    require(0 <= header_end < META_LIMIT, 'bounded LF header required')
    header = parse_json(raw[:header_end], 'header')
    exact(header, expected_header, 'header')
    start = header_end + 1
    end = start + expected_bytes
    require(raw[end:end + 1] == b'\n', 'exact payload separator required')
    footer_raw = raw[end + 1:]
    require(0 < len(footer_raw) <= META_LIMIT and footer_raw.endswith(b'\n')
            and footer_raw.count(b'\n') == 1, 'bounded footer, no trailing junk required')
    footer = parse_json(footer_raw[:-1], 'footer')
    payload = raw[start:end]
    require(len(payload) == expected_bytes, 'complete used payload required')
    digest = hashlib.sha256(payload).hexdigest()
    exact(footer, {'complete': True, 'payload_bytes': expected_bytes,
                   'sha256': digest}, 'footer')
    exact(export_outcome['payload_bytes'], expected_bytes, 'outcome payload bytes')
    exact(export_outcome['sha256'], digest, 'outcome payload digest')
    return payload


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for option in ['source', 'expected-source-sha256', 'matrix-export',
                   'matrix-outcome', 'solution-export', 'solution-outcome']:
        parser.add_argument('--' + option, required=True)
    args = parser.parse_args()
    source_path = Path(args.source)
    source_raw = read_bounded(source_path, 65536, 'source')
    source_sha = hashlib.sha256(source_raw).hexdigest()
    require(re.fullmatch(r'[0-9a-fA-F]{64}', args.expected_source_sha256) is not None,
            'expected source SHA-256 required')
    require(source_sha == args.expected_source_sha256.lower(), 'initial source hash mismatch')
    core = load_core()
    matrix_source, matrix_outcome, matrix_iterations = outcome(
        args.matrix_outcome, source_path, source_sha, len(source_raw), 'A')
    solution_source, solution_outcome, solution_iterations = outcome(
        args.solution_outcome, source_path, source_sha, len(source_raw), 'solution')
    exact(matrix_source, solution_source, 'two command source identity')
    sections = [
        {'name': 'column_pointers', 'encoding': 'u32-le', 'elements': N + 1,
         'bytes': 131076, 'offset_bytes': 0},
        {'name': 'row_indices', 'encoding': 'u32-le', 'elements': NNZ,
         'bytes': 892928, 'offset_bytes': 131076},
        {'name': 'values', 'encoding': 'ieee754-binary64-le', 'elements': NNZ,
         'components': 1, 'bytes': 1785856, 'offset_bytes': 1024004}]
    matrix_header = {'format': 'onl-scientific-csc-v1', 'class': 'double',
                     'complex': False, 'shape': [N, N], 'logical_elements': N * N,
                     'nnz': NNZ, 'nzmax': NNZ, 'index_base': 0, 'storage': 'csc',
                     'sections': sections, 'payload_bytes': 2809860,
                     'binding': 'A', 'source_sha256': source_sha}
    solution_header = {'format': 'onl-scientific-array-v1', 'class': 'double',
                       'complex': False, 'shape': [N, 1], 'elements': N,
                       'component_encoding': 'ieee754-binary64-le',
                       'storage': 'column-major-interleaved', 'binding': 'solution',
                       'source_sha256': source_sha}
    matrix = export_file(args.matrix_export, matrix_header, matrix_outcome, 2809860)
    solution = export_file(args.solution_export, solution_header, solution_outcome, N * 8)
    pointers = [v[0] for v in struct.iter_unpack('<I', matrix[:131076])]
    indices = [v[0] for v in struct.iter_unpack('<I', matrix[131076:1024004])]
    matrix_words = [v[0] for v in struct.iter_unpack('<Q', matrix[1024004:])]
    solution_words = [v[0] for v in struct.iter_unpack('<Q', solution)]
    csc_result = core.check_csc(pointers, indices, matrix_words,
                              core.ValueDescriptor('double', (N, N), False, 'CSC'), n=32)
    solution_result = core.check_solution(solution_words,
                              core.ValueDescriptor('double', (N, 1), False, 'column-major'), n=32)
    require(read_bounded(source_path, 65536, 'source') == source_raw,
            'source changed during validation')
    print(json.dumps({'status': 'passed', 'source_sha256': source_sha,
                      'core_sha256': CORE_SHA256, 'flags': {'matrix': 0, 'solution': 0},
                      'iterations': {'matrix': matrix_iterations, 'solution': solution_iterations},
                      'csc': csc_result, 'solution': solution_result}, separators=(',', ':')))


if __name__ == '__main__':
    try:
        main()
    except (ValueError, OSError, OverflowError, struct.error) as error:
        print(json.dumps({'status': 'failed', 'error': str(error)}, separators=(',', ':')),
              file=sys.stderr)
        raise SystemExit(1)
