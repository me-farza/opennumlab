// SPDX-License-Identifier: GPL-3.0-or-later
// Dense complex components for the owned worker; selected ABI support only.
// Preserve dense complex component bits without Octave scalar/index narrowing.
// No interpreter evaluation, overloaded indexing, callback, base assignment,
// registry mutation, graphics access or persistent numeric ownership occurs.
#include <octave/oct.h>
#include <octave/defun-dld.h>
#include <octave/ov-cell.h>
#include <octave/ov-complex.h>
#include <octave/ov-cx-mat.h>
#include <octave/ov-flt-complex.h>
#include <octave/ov-flt-cx-mat.h>
#include <octave/ov-float.h>
#include <octave/ov-flt-re-mat.h>
#include <octave/ov-range.h>
#include <octave/ov-re-mat.h>
#include <octave/ov-scalar.h>
#include <octave/ov-str-mat.h>
#include <octave/quit.h>
#include <cmath>
#include <complex>
#include <cstdint>
#include <cstring>
#include <initializer_list>
#include <limits>
#include <string>
#include <type_traits>
#include <vector>

namespace {
constexpr octave_idx_type max_cells = 2000;
constexpr int max_rank = 32;
constexpr std::uint64_t first_unsupported_index = UINT64_C(9007199254740992);
constexpr std::uint64_t max_replace_leaf_bytes = UINT64_C(134217728);
static_assert(sizeof(float) == 4 && std::numeric_limits<float>::is_iec559,
              "The precision bridge requires IEEE binary32");
static_assert(sizeof(double) == 8 && std::numeric_limits<double>::is_iec559,
              "The precision bridge requires IEEE binary64");

bool dense_double_complex(const octave_value& value) {
  return value.type_id() == octave_complex_matrix::static_type_id()
      || value.type_id() == octave_complex::static_type_id();
}
bool dense_single_complex(const octave_value& value) {
  return value.type_id() == octave_float_complex_matrix::static_type_id()
      || value.type_id() == octave_float_complex::static_type_id();
}
bool genuine_double_real(const octave_value& value, bool ranges = false) {
  return value.type_id() == octave_matrix::static_type_id()
      || value.type_id() == octave_scalar::static_type_id()
      || (ranges && value.type_id() == ov_range<double>::static_type_id());
}
bool genuine_single_real(const octave_value& value) {
  return value.type_id() == octave_float_matrix::static_type_id()
      || value.type_id() == octave_float_scalar::static_type_id();
}

struct selection {
  dim_vector source_shape;
  octave_idx_type source_cells = 0;
  octave_idx_type rows = 0, columns = 0;
  std::vector<octave_idx_type> offsets;
};

// Exact native type IDs are checked before dims/array extraction. In particular
// neither a classdef getter nor an old-style object's conversion can run here.
selection select(const octave_value& source, const octave_value& encoded,
                 bool writing) {
  if (!dense_double_complex(source) && !dense_single_complex(source))
    error_with_id("MathLab:ValueBridgeType",
                  "Expected a genuine dense complex single/double leaf; sparse, diagonal, real and object values are unsupported");
  const int rank = source.ndims();
  if (rank < 2 || rank > max_rank)
    error_with_id("MathLab:ValueBridgeLimit", "Source rank must be between 2 and 32");
  const dim_vector shape = source.dims();
  bool empty = false;
  for (int axis = 0; axis < rank; ++axis) {
    const auto extent = shape(axis);
    if (extent < 0 || static_cast<std::uint64_t>(extent) >= first_unsupported_index)
      error_with_id("MathLab:ValueBridgeLimit", "Each source extent must be below 2^53");
    empty = empty || extent == 0;
  }
  std::uint64_t total = empty ? 0 : 1;
  if (!empty) {
    for (int axis = 0; axis < rank; ++axis) {
      const auto extent = static_cast<std::uint64_t>(shape(axis));
      const auto maximum = static_cast<std::uint64_t>(std::numeric_limits<octave_idx_type>::max());
      if (total > maximum / extent)
        error_with_id("MathLab:ValueBridgeLimit", "Source shape product exceeds native indexing");
      total *= extent;
    }
  }
  if (source.numel() < 0 || static_cast<std::uint64_t>(source.numel()) != total)
    error_with_id("MathLab:ValueBridgeType", "Source shape and native element count disagree");
  if (encoded.type_id() != octave_cell::static_type_id()
      || encoded.numel() != rank || encoded.ndims() != 2)
    error_with_id("MathLab:ValueBridgeSelection", "Provide exactly one genuine cell subscript per source axis");
  const Cell subscripts = encoded.cell_value(); // at most32 references, no data copy
  std::vector<octave_idx_type> starts(static_cast<std::size_t>(rank), 0);
  std::vector<octave_idx_type> counts(static_cast<std::size_t>(rank), 0);
  for (int axis = 0; axis < rank; ++axis) {
    const octave_value& input = subscripts(axis);
    if (!genuine_double_real(input, true))
      error_with_id("MathLab:ValueBridgeSelection", "Subscripts must be genuine real double vectors/ranges");
    const auto count = input.numel();
    const auto budget = axis < 2 ? max_cells : 1;
    if (count < 0 || count > budget || (axis >= 2 && count != 1))
      error_with_id("MathLab:ValueBridgeLimit", "Subscripts exceed the bounded plane selection");
    if (input.ndims() != 2)
      error_with_id("MathLab:ValueBridgeSelection", "Each subscript must be a 2-D vector");
    const dim_vector index_shape = input.dims();
    if (count > 0 && index_shape(0) != 1 && index_shape(1) != 1)
      error_with_id("MathLab:ValueBridgeSelection", "Each subscript must be a vector");
    if (count == 0) {
      if (writing || axis >= 2 || shape(axis) != 0)
        error_with_id("MathLab:ValueBridgeSelection", "Only a zero-length source row/column axis permits an empty read selection");
      continue;
    }
    if (shape(axis) == 0)
      error_with_id("MathLab:ValueBridgeSelection", "A zero-length source axis has no positive selectable index");
    // Materialize only AFTER proving this is a builtin real type with <=2000
    // entries. No whole numeric source or user conversion is materialized.
    const NDArray values = input.array_value();
    const double *data = values.data();
    octave_idx_type first = 0;
    for (octave_idx_type entry = 0; entry < count; ++entry) {
      const double number = data[entry];
      if (!std::isfinite(number) || number < 1 || std::floor(number) != number
          || number >= static_cast<double>(first_unsupported_index)
          || number > static_cast<double>(std::numeric_limits<octave_idx_type>::max()))
        error_with_id("MathLab:ValueBridgeSelection", "Subscripts require exact positive indices below 2^53");
      const auto index = static_cast<octave_idx_type>(number);
      if (index > shape(axis))
        error_with_id("MathLab:ValueBridgeSelection", "The entire selection must fit the source without clipping");
      if (entry == 0) first = index;
      else if (index != first + entry)
        error_with_id("MathLab:ValueBridgeSelection", "The first two axes must be contiguous increasing ranges");
    }
    starts[static_cast<std::size_t>(axis)] = first - 1;
    counts[static_cast<std::size_t>(axis)] = count;
  }
  const auto rows = counts[0], columns = counts[1];
  if ((writing && (rows == 0 || columns == 0))
      || (columns != 0 && rows > max_cells / columns))
    error_with_id("MathLab:ValueBridgeLimit", "A selected plane contains at most 2000 cells; writes must be nonempty");
  const auto cells = rows * columns; // each factor<=2000 and product admitted
  selection result {shape, static_cast<octave_idx_type>(total), rows, columns, {}};
  result.offsets.reserve(static_cast<std::size_t>(cells));
  if (cells == 0) return result; // do not overflow strides for an ultimately empty shape
  octave_idx_type plane = 0, stride = shape(0) * shape(1);
  for (int axis = 2; axis < rank; ++axis) {
    // Total shape product was checked against octave_idx_type before this math.
    plane += starts[static_cast<std::size_t>(axis)] * stride;
    stride *= shape(axis);
  }
  for (octave_idx_type column = 0; column < columns; ++column)
    for (octave_idx_type row = 0; row < rows; ++row)
      result.offsets.push_back(plane + starts[0] + row + (starts[1] + column) * shape(0));
  return result;
}

struct double_types {
  using real = double;
  using complex = Complex;
  using real_array = NDArray;
  using complex_array = ComplexNDArray;
  static const complex *data(const octave_value& value) {
    if (value.type_id() == octave_complex::static_type_id())
      return &static_cast<const octave_complex&>(value.get_rep()).scalar_ref();
    return static_cast<const octave_complex_matrix&>(value.get_rep()).matrix_ref().data();
  }
  static complex_array cow_array(const octave_value& value, const selection& selected) {
    if (value.type_id() == octave_complex_matrix::static_type_id())
      return static_cast<const octave_complex_matrix&>(value.get_rep()).matrix_ref();
    complex_array result(selected.source_shape);
    std::memcpy(result.rwdata(), data(value), sizeof(complex));
    return result;
  }
  static bool real_type(const octave_value& value) { return genuine_double_real(value); }
  static real_array real_values(const octave_value& value) { return value.array_value(); }
  static octave_value wrap_real(const real_array& values) { return octave_value(new octave_matrix(values)); }
  static octave_value wrap_complex(const complex_array& values) { return octave_value(new octave_complex_matrix(values)); }
};
struct single_types {
  using real = float;
  using complex = FloatComplex;
  using real_array = FloatNDArray;
  using complex_array = FloatComplexNDArray;
  static const complex *data(const octave_value& value) {
    if (value.type_id() == octave_float_complex::static_type_id())
      return &static_cast<const octave_float_complex&>(value.get_rep()).scalar_ref();
    return static_cast<const octave_float_complex_matrix&>(value.get_rep()).matrix_ref().data();
  }
  static complex_array cow_array(const octave_value& value, const selection& selected) {
    if (value.type_id() == octave_float_complex_matrix::static_type_id())
      return static_cast<const octave_float_complex_matrix&>(value.get_rep()).matrix_ref();
    complex_array result(selected.source_shape);
    std::memcpy(result.rwdata(), data(value), sizeof(complex));
    return result;
  }
  static bool real_type(const octave_value& value) { return genuine_single_real(value); }
  static real_array real_values(const octave_value& value) { return value.float_array_value(); }
  static octave_value wrap_real(const real_array& values) { return octave_value(new octave_float_matrix(values)); }
  static octave_value wrap_complex(const complex_array& values) { return octave_value(new octave_float_complex_matrix(values)); }
};

// std::complex array-oriented access exposes its two adjacent component slots.
// memcpy avoids floating arithmetic, implicit width conversion and signaling
// NaN quieting in this adapter. Unsupported object layouts fail at compile time.
template <typename Types>
struct components {
  using real = typename Types::real;
  using complex = typename Types::complex;
  using array = typename Types::real_array;
  static_assert(sizeof(complex) == 2 * sizeof(real), "Expected two adjacent complex components");
  static_assert(std::is_trivially_copyable<complex>::value, "Complex storage must support bit copies");
  array re, im;
  components(octave_idx_type rows, octave_idx_type columns)
    : re(dim_vector(rows, columns)), im(dim_vector(rows, columns)) {}
  octave_value as_value() const {
    octave_scalar_map result;
    result.assign("real", Types::wrap_real(re));
    result.assign("imag", Types::wrap_real(im));
    return octave_value(result);
  }
};

template <typename Types>
components<Types> capture(const octave_value& source, const selection& selected) {
  using real = typename Types::real;
  components<Types> result(selected.rows, selected.columns);
  const auto *data = Types::data(source);
  auto *re = result.re.rwdata();
  auto *im = result.im.rwdata();
  for (std::size_t index = 0; index < selected.offsets.size(); ++index) {
    OCTAVE_QUIT;
    const real *parts = reinterpret_cast<const real *>(data + selected.offsets[index]);
    std::memcpy(re + index, parts, sizeof(real));
    std::memcpy(im + index, parts + 1, sizeof(real));
  }
  return result; // fresh selected-only arrays; no source backing allocation retained
}

template <typename Types>
octave_value read(const octave_value& source, const selection& selected) {
  return capture<Types>(source, selected).as_value();
}

template <typename Types>
octave_value_list replace(const octave_value& source, const selection& selected,
                          const octave_value& real_input, const octave_value& imag_input) {
  using real = typename Types::real;
  using complex = typename Types::complex;
  if (static_cast<std::uint64_t>(selected.source_cells) > max_replace_leaf_bytes / sizeof(complex))
    error_with_id("MathLab:ValueBridgeLimit", "Replacement source exceeds 128 MiB logical complex leaf admission");
  for (const auto *input : {&real_input, &imag_input}) {
    if (!Types::real_type(*input))
      error_with_id("MathLab:ValueBridgeType", "Replacement components must be genuine dense real arrays of the source precision");
    if (input->ndims() != 2 || input->rows() != selected.rows
        || input->columns() != selected.columns
        || input->numel() != static_cast<octave_idx_type>(selected.offsets.size()))
      error_with_id("MathLab:ValueBridgeSelection", "Replacement real/imag shapes must exactly equal the selected rows and columns");
  }
  const auto re_input = Types::real_values(real_input);
  const auto im_input = Types::real_values(imag_input);
  components<Types> after(selected.rows, selected.columns);
  const auto bytes = selected.offsets.size() * sizeof(real);
  std::memcpy(after.re.rwdata(), re_input.data(), bytes);
  std::memcpy(after.im.rwdata(), im_input.data(), bytes);
  auto before = capture<Types>(source, selected);
  // Build every bounded return object before the full leaf COW allocation.
  const octave_value before_value = before.as_value();
  const octave_value after_value = after.as_value();
  OCTAVE_QUIT;
  auto replacement = Types::cow_array(source, selected);
  auto *destination = replacement.rwdata(); // COW detaches only admitted full logical leaf
  const auto *re = after.re.data(), *im = after.im.data();
  for (std::size_t index = 0; index < selected.offsets.size(); ++index) {
    OCTAVE_QUIT;
    real *parts = reinterpret_cast<real *>(destination + selected.offsets[index]);
    std::memcpy(parts, re + index, sizeof(real));
    std::memcpy(parts + 1, im + index, sizeof(real));
  }
  // Pointer-wrapper construction deliberately bypasses octave_value's normal
  // complex-array narrowing constructor, even for1x1 or all-real components.
  // The returned node is a genuine builtin complex matrix; no custom type is
  // registered and no module-owned callback/destructor escapes this function.
  return ovl(Types::wrap_complex(replacement), before_value, after_value);
}

std::string operation_name(const octave_value& value) {
  if ((value.type_id() != octave_char_matrix_str::static_type_id()
       && value.type_id() != octave_char_matrix_sq_str::static_type_id())
      || value.ndims() != 2 || value.rows() != 1 || value.numel() > 32)
    error_with_id("MathLab:ValueBridgeArguments", "Expected a bounded operation name");
  return value.string_value();
}
} // namespace

DEFUN_DLD(mathlab_value_bridge, args, nargout,
          "Narrow dense complex component access for a private MathLab worker.") {
  if (args.length() < 1)
    error_with_id("MathLab:ValueBridgeArguments", "Expected an operation");
  const auto operation = operation_name(args(0));
  if (operation == "protocol" && args.length() == 1 && nargout <= 1) {
    octave_scalar_map result;
    result.assign("protocol", "mathlab-value");
    result.assign("version", 1);
    result.assign("dense_complex_components", true);
    result.assign("max_cells", static_cast<double>(max_cells));
    result.assign("max_rank", max_rank);
    result.assign("max_replace_leaf_bytes", static_cast<double>(max_replace_leaf_bytes));
    return ovl(result);
  }
  if (operation == "read_components" && args.length() == 3 && nargout <= 1) {
    const auto selected = select(args(1), args(2), false);
    if (dense_single_complex(args(1))) return ovl(read<single_types>(args(1), selected));
    return ovl(read<double_types>(args(1), selected));
  }
  if (operation == "replace_components" && args.length() == 5 && nargout <= 3) {
    const auto selected = select(args(1), args(2), true);
    if (dense_single_complex(args(1))) return replace<single_types>(args(1), selected, args(3), args(4));
    return replace<double_types>(args(1), selected, args(3), args(4));
  }
  error_with_id("MathLab:ValueBridgeArguments", "Unknown operation, argument count or output count");
  return octave_value_list();
}
