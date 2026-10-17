## SPDX-License-Identifier: GPL-3.0-or-later
## Standalone provider fixture; append the exact shared mathlab_intrinsics.m.
## Arg2 is an internal handle to the freshly built mathlab_lock_probe DLD.
## Arg3 selects seed, clear_functions, clear_all or reentry. Other transport
## arguments are placeholders. This fixture never replaces the real worker.
function mathlab_worker (session, probe, action, a4, a5, a6, a7, a8, a9, a10, bindings, initial_directory)
  d = bindings.dispatcher;
  d('lastwarn', '');
  before = probe (bindings.caller_lock);
  if (d('strcmp', action, 'seed'))
    if (before.keeper_locked)
      d('error', 'MathLabTest:KeeperCold', 'Fresh actual keeper was already locked.');
    endif
  else
    ## Inspect retained state before the worker's normal reseeding can mask loss.
    [prior, prior_bridge, prior_adapters] = mathlab_intrinsics ();
    if (! before.keeper_locked || ! d('isequal', prior, d) ...
        || ! d('isequal', prior_bridge, bindings.bridge) ...
        || ! d('isequal', prior_adapters, bindings.adapters))
      d('error', 'MathLabTest:KeeperReentry', 'Same-object reentry lost private binding state.');
    endif
  endif
  ## Isolate the real keeper call from assertion helpers such as isequal.m,
  ## whose disabled language-extension diagnostics can still update lastwarn.
  d('lastwarn', '');
  mathlab_intrinsics (1, bindings);
  seed_warning = d('lastwarn');
  if (! d('isempty', seed_warning))
    d('error', 'MathLabTest:KeeperSeedWarning', 'Private keeper seed emitted a warning: %s', seed_warning);
  endif
  [retained, bridge, adapters] = mathlab_intrinsics ();
  after_seed = probe (bindings.caller_lock);
  if (! after_seed.direct_actual_mlock || ! after_seed.keeper_locked ...
      || after_seed.registry_locked || ! d('isequal', retained, d))
    d('error', 'MathLabTest:KeeperLock', 'Seed did not lock only the actual private keeper.');
  endif
  ## Short-lived scope helper releases the actual registry handle on return.
  mathlab_scope_call ('__ml_lock_probe_key__', {73});
  if (! d('isequal', mathlab_scope_call ('__ml_lock_probe_key__', {}, 'lookup'), {73}))
    d('error', 'MathLabTest:RegistrySeed', 'Numerical registry marker was not stored.');
  endif
  d('lastwarn', '');
  switch (action)
    case {'seed', 'reentry'}
      ## Reseeding must remain harmless on the same actual objects.
      mathlab_intrinsics (1, bindings);
    case 'clear_functions'
      d('evalin', 'base', 'clear functions;');
    case 'clear_all'
      d('evalin', 'base', 'clear all;');
    otherwise
      d('error', 'MathLabTest:LockAction', 'Unknown fixed lock fixture action.');
  endswitch
  action_warning = d('lastwarn');
  if (! d('isempty', action_warning))
    d('error', 'MathLabTest:KeeperActionWarning', 'Private keeper action emitted a warning: %s', action_warning);
  endif
  ## Do not seed again after clear: retained state itself must survive.
  [retained, retained_bridge, retained_adapters] = mathlab_intrinsics ();
  if (! d('isequal', retained, d) || ! d('isequal', retained_bridge, bridge) ...
      || ! d('isequal', retained_adapters, adapters))
    d('error', 'MathLabTest:KeeperClear', 'Clear discarded private binding state.');
  endif
  ## Numerical registry is intentionally re-established after ordinary clearing.
  marker = mathlab_scope_call ('__ml_lock_probe_key__', {}, 'lookup');
  cleared = d('any', d('strcmp', action, {'clear_functions', 'clear_all'}));
  if ((cleared && ! d('isempty', marker)) ...
      || (! cleared && ! d('isequal', marker, {73})))
    d('error', 'MathLabTest:RegistryClear', 'Numerical registry persistent state did not follow ordinary clear.');
  endif
  after_action = probe (bindings.caller_lock);
  assertion_warning = d('lastwarn');
  if (! after_action.keeper_locked || after_action.registry_locked)
    d('error', 'MathLabTest:KeeperLock', 'Actual keeper or numerical registry lock invariant failed.');
  endif
  report = d('struct', 'action', action, 'before', before, ...
    'after_seed', after_seed, 'after_action', after_action, ...
    'registry_marker_cleared', d('isempty', marker), ...
    'seed_warning', seed_warning, 'action_warning', action_warning, ...
    'assertion_lastwarn', assertion_warning);
  d('fprintf', 1, 'MATHLAB_CALLER_LOCK %s\n', d('jsonencode', report));
endfunction
