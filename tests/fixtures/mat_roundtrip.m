% Version 7 numeric, complex, logical, sparse, char, cell, and struct round trip.
% This does not establish MATLAB reference compatibility for every MAT type.
% SPDX-License-Identifier: GPL-3.0-or-later
fixture_path = [tempname(), '.mat'];
fixture_numeric = reshape(1:24, [2, 3, 4]);
fixture_complex = [1+2i; 3-4i];
fixture_logical = logical([1, 0; 0, 1]);
fixture_sparse = sparse([1, 2], [2, 1], [3, 4], 2, 2);
fixture_char = 'MathLab round trip';
fixture_cell = {fixture_numeric, fixture_char};
fixture_struct = struct('samples', fixture_numeric, 'label', fixture_char);
fixture_u8 = uint8([0, 128, 255]);
try
    save(fixture_path, 'fixture_numeric', 'fixture_complex', 'fixture_logical', ...
        'fixture_sparse', 'fixture_char', 'fixture_cell', 'fixture_struct', 'fixture_u8', '-v7');
    fixture_loaded = load(fixture_path);
    assert(isequal(fixture_loaded.fixture_numeric, fixture_numeric));
    assert(isa(fixture_loaded.fixture_numeric, 'double'));
    assert(isequal(fixture_loaded.fixture_complex, fixture_complex));
    assert(~isreal(fixture_loaded.fixture_complex));
    assert(islogical(fixture_loaded.fixture_logical));
    assert(isequal(fixture_loaded.fixture_logical, fixture_logical));
    assert(issparse(fixture_loaded.fixture_sparse));
    assert(isequal(fixture_loaded.fixture_sparse, fixture_sparse));
    assert(isequal(fixture_loaded.fixture_char, fixture_char));
    assert(ischar(fixture_loaded.fixture_char));
    assert(iscell(fixture_loaded.fixture_cell));
    assert(isequal(fixture_loaded.fixture_cell, fixture_cell));
    assert(isstruct(fixture_loaded.fixture_struct));
    assert(isequal(fixture_loaded.fixture_struct, fixture_struct));
    assert(isa(fixture_loaded.fixture_u8, 'uint8'));
    assert(isequal(fixture_loaded.fixture_u8, fixture_u8));
catch fixture_error
    if exist(fixture_path, 'file')
        delete(fixture_path);
    end
    rethrow(fixture_error);
end
delete(fixture_path);
disp('PASS mat_roundtrip');
