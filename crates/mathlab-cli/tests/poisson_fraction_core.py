"""Independent exact oracle for the real 32^3 Dirichlet Poisson workload.

No production assembler, SpMV, PCG, Rust output, or export-wire parser is used.
The smaller n parameter exists for manufactured selfchecks, not tolerance tuning.
"""
from dataclasses import dataclass
from fractions import Fraction

BACKWARD_LIMIT = Fraction(1, 1 << 36)
FORWARD_LIMIT = Fraction(1, 1 << 24)
SIX_WORD = 0x4018000000000000
MINUS_ONE_WORD = 0xBFF0000000000000


class CheckFailure(ValueError):
    pass


@dataclass(frozen=True)
class ValueDescriptor:
    class_name: str
    shape: tuple
    complex: bool
    storage_order: str


def _size(n):
    if type(n) is not int or not 1 <= n <= 32:
        raise CheckFailure("n must be an integer in 1..32; production n is 32")
    return n ** 3


def _descriptor(value, shape, storage):
    if not isinstance(value, ValueDescriptor):
        raise CheckFailure("explicit typed descriptor required")
    if value.class_name != "double" or value.complex is not False:
        raise CheckFailure("real double required")
    if (not isinstance(value.shape, tuple)
            or any(type(x) is not int for x in value.shape)
            or value.shape != shape):
        raise CheckFailure("exact canonical shape required")
    if value.storage_order != storage:
        raise CheckFailure("storage order mismatch")


def word_bits(word):
    """Accept a u64 or exactly sixteen hexadecimal digits; never round a word."""
    if type(word) is int and 0 <= word < 1 << 64:
        return word
    if (isinstance(word, str) and len(word) == 16
            and all(c in "0123456789abcdefABCDEF" for c in word)):
        return int(word, 16)
    raise CheckFailure("expected one binary64 word")


def finite_fraction(word):
    bits = word_bits(word)
    sign = -1 if bits >> 63 else 1
    exponent = (bits >> 52) & 0x7FF
    fraction = bits & ((1 << 52) - 1)
    if exponent == 0x7FF:
        raise CheckFailure("nonfinite returned binary64 word")
    if exponent == 0:
        mantissa, shift = fraction, -1074
    else:
        mantissa, shift = (1 << 52) + fraction, exponent - 1075
    if shift >= 0:
        return Fraction(sign * (mantissa << shift))
    return Fraction(sign * mantissa, 1 << -shift)


def _coordinate(index, n):
    k0, rest = divmod(index, n * n)
    j0, i0 = divmod(rest, n)
    return i0 + 1, j0 + 1, k0 + 1


def _neighbors(index, n, coordinate):
    i, j, k = coordinate
    if i > 1:
        yield index - 1
    if i < n:
        yield index + 1
    if j > 1:
        yield index - n
    if j < n:
        yield index + n
    if k > 1:
        yield index - n * n
    if k < n:
        yield index + n * n


def _ratio(value):
    return {"numerator": str(value.numerator),
            "denominator": str(value.denominator)}


def check_solution(words, descriptor, *, n=32):
    """Check every x word, true stencil residual, backward and forward error.

    The input is the complete real column vector, not a preview or recursively
    updated residual. Binary64 inputs are converted to exact dyadic rationals.
    """
    count = _size(n)
    _descriptor(descriptor, (count, 1), "column-major")
    if len(words) != count:
        raise CheckFailure("complete solution payload required")
    values = [finite_fraction(word) for word in words]
    residual_norm = Fraction(0)
    returned_norm = Fraction(0)
    rhs_norm = 0
    error_norm = Fraction(0)
    exact_norm = 0
    matrix_norm = 0
    worst_residual_index = 0
    worst_error_index = 0
    for index, returned in enumerate(values):
        coordinate = _coordinate(index, n)
        qi, qj, qk = (t * (n + 1 - t) for t in coordinate)
        exact = qi * qj * qk
        rhs = 2 * (qj * qk + qi * qk + qi * qj)
        neighbors = tuple(_neighbors(index, n, coordinate))
        # This geometric stencil is independent of the returned CSC and of
        # production matrix multiplication. Boundary values are exactly zero.
        ax = 6 * returned - sum((values[j] for j in neighbors), Fraction(0))
        residual = abs(rhs - ax)
        error = abs(returned - exact)
        if residual > residual_norm:
            residual_norm, worst_residual_index = residual, index
        if error > error_norm:
            error_norm, worst_error_index = error, index
        returned_norm = max(returned_norm, abs(returned))
        rhs_norm = max(rhs_norm, abs(rhs))
        exact_norm = max(exact_norm, abs(exact))
        matrix_norm = max(matrix_norm, 6 + len(neighbors))
    denominator = matrix_norm * returned_norm + rhs_norm
    # rhs and xstar are nonzero for every admitted n; never mask a 0/0 case.
    if denominator <= 0 or exact_norm <= 0:
        raise CheckFailure("invalid normalization")
    eta = residual_norm / denominator
    forward = error_norm / exact_norm
    if eta > BACKWARD_LIMIT:
        raise CheckFailure("true backward error exceeds 2^-36")
    if forward > FORWARD_LIMIT:
        raise CheckFailure("relative forward error exceeds 2^-24")
    return {"n": n, "returned_words_checked": count,
            "matrix_infinity_norm": matrix_norm,
            "residual_infinity_norm": _ratio(residual_norm),
            "returned_infinity_norm": _ratio(returned_norm),
            "rhs_infinity_norm": _ratio(Fraction(rhs_norm)),
            "exact_infinity_norm": _ratio(Fraction(exact_norm)),
            "error_infinity_norm": _ratio(error_norm),
            "eta": _ratio(eta), "forward": _ratio(forward),
            "backward_limit": _ratio(BACKWARD_LIMIT),
            "forward_limit": _ratio(FORWARD_LIMIT),
            "worst_residual_index_zero_based": worst_residual_index,
            "worst_error_index_zero_based": worst_error_index}


def check_csc(column_pointers, row_indices, value_words, descriptor, *, n=32):
    """Verify every canonical CSC position and word by geometric neighbors.

    No triplet constructor, production SpMV, or N-by-N dense image is used.
    Pointer/row inputs are already decoded zero-based integers. Wire integrity,
    capacities and typed decoding are separate adapter obligations.
    """
    count = _size(n)
    _descriptor(descriptor, (count, count), "CSC")
    expected_nnz = count + 6 * (n - 1) * n * n
    if (len(column_pointers) != count + 1
            or len(row_indices) != expected_nnz
            or len(value_words) != expected_nnz):
        raise CheckFailure("complete CSC arrays with exact lengths required")
    offset = 0
    for column in range(count):
        coordinate = _coordinate(column, n)
        expected_rows = sorted((column, *_neighbors(column, n, coordinate)))
        pointer = column_pointers[column]
        if type(pointer) is not int or pointer != offset:
            raise CheckFailure(f"CSC start pointer mismatch at column {column}")
        for row in expected_rows:
            actual_row = row_indices[offset]
            if type(actual_row) is not int or actual_row != row:
                raise CheckFailure(f"CSC row mismatch at entry {offset}")
            expected_word = SIX_WORD if row == column else MINUS_ONE_WORD
            if word_bits(value_words[offset]) != expected_word:
                raise CheckFailure(f"CSC value mismatch at entry {offset}")
            offset += 1
        pointer = column_pointers[column + 1]
        if type(pointer) is not int or pointer != offset:
            raise CheckFailure(f"CSC end pointer mismatch at column {column}")
    if offset != expected_nnz:
        raise CheckFailure("CSC complete-entry count mismatch")
    return {"n": n, "logical_shape": [count, count],
            "column_pointers_checked": count + 1,
            "stored_entries_checked": offset,
            "canonical_geometric_stencil": True}
