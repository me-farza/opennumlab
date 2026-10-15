// SPDX-License-Identifier: GPL-3.0-or-later
// Deliberately independent of shared desktop wiring and egui/native libraries.
#[path = "../src/file_actions.rs"]
mod file_actions;

use file_actions::{
    action_for_key, intent_for_key, FileAction, FileKey, PhysicalModifiers, Platform,
};

fn modifiers(mask: u8) -> PhysicalModifiers {
    PhysicalModifiers {
        control: mask & 1 != 0,
        command: mask & 2 != 0,
        shift: mask & 4 != 0,
        alt: mask & 8 != 0,
    }
}

#[test]
fn documented_bindings_and_hints_use_the_selected_platform() {
    for (platform, required, prefix) in [
        (Platform::MacOs, modifiers(2), "Command"),
        (Platform::Windows, modifiers(1), "Ctrl"),
        (Platform::Linux, modifiers(1), "Ctrl"),
    ] {
        for (action, key, suffix) in [
            (FileAction::Open, FileKey::O, "O"),
            (FileAction::Save, FileKey::S, "S"),
        ] {
            let descriptor = action.documented_binding(platform).unwrap();
            assert_eq!(descriptor.key(), key);
            assert_eq!(descriptor.modifiers(), required);
            assert_eq!(descriptor.menu_hint(), format!("{prefix}+{suffix}"));
            assert_eq!(action.menu_hint(platform), Some(descriptor.menu_hint()));
        }
    }
}

#[test]
fn all_physical_modifier_combinations_have_only_the_two_exact_matches() {
    // Test expectations are independent literal masks, not read from descriptors.
    for (platform, required_mask) in [
        (Platform::MacOs, 2),
        (Platform::Windows, 1),
        (Platform::Linux, 1),
    ] {
        for mask in 0..16 {
            for (key, exact_action) in [
                (FileKey::O, Some(FileAction::Open)),
                (FileKey::S, Some(FileAction::Save)),
                (FileKey::Other, None),
            ] {
                let expected = if mask == required_mask {
                    exact_action
                } else {
                    None
                };
                assert_eq!(
                    action_for_key(platform, key, modifiers(mask)),
                    expected,
                    "platform={platform:?} key={key:?} physical_mask={mask}"
                );
            }
        }
    }
}

#[test]
fn command_shift_s_cannot_fall_through_to_save_or_invent_save_as() {
    for (platform, exact, shifted) in [
        (Platform::MacOs, modifiers(2), modifiers(2 | 4)),
        (Platform::Windows, modifiers(1), modifiers(1 | 4)),
        (Platform::Linux, modifiers(1), modifiers(1 | 4)),
    ] {
        let save = FileAction::Save.documented_binding(platform).unwrap();
        assert!(save.matches(FileKey::S, exact));
        assert!(!save.matches(FileKey::S, shifted));
        assert!(intent_for_key(platform, FileKey::S, shifted, 73_usize).is_none());
        assert!(FileAction::SaveAs.documented_binding(platform).is_none());
        assert!(FileAction::SaveAs.menu_hint(platform).is_none());
    }
}

#[test]
fn macos_command_and_literal_control_are_independent() {
    assert_eq!(
        action_for_key(Platform::MacOs, FileKey::S, modifiers(2)),
        Some(FileAction::Save)
    );
    assert_eq!(
        action_for_key(Platform::MacOs, FileKey::S, modifiers(1)),
        None
    );
    assert_eq!(
        action_for_key(Platform::MacOs, FileKey::S, modifiers(1 | 2)),
        None
    );
    for platform in [Platform::Windows, Platform::Linux] {
        assert_eq!(action_for_key(platform, FileKey::S, modifiers(2)), None);
        assert_eq!(action_for_key(platform, FileKey::S, modifiers(1 | 2)), None);
    }
}

#[test]
fn semantic_action_ids_are_stable_and_distinct() {
    assert_eq!(
        FileAction::ALL.map(FileAction::id),
        ["file.open", "file.save", "file.save_as"]
    );
}

#[test]
fn keyboard_intent_keeps_the_explicit_document_identity() {
    #[derive(Debug, PartialEq, Eq)]
    struct DocumentIdentity {
        session: String,
        document: usize,
    }
    let selected = DocumentIdentity {
        session: "original-session".to_owned(),
        document: 73,
    };
    // Non-Copy identities are supported; there is no Clone requirement or lookup.
    let intent = intent_for_key(Platform::MacOs, FileKey::S, modifiers(2), selected).unwrap();
    assert_eq!(intent.action(), FileAction::Save);
    assert_eq!(intent.target_document().document, 73);
    assert_eq!(intent.target_document().session, "original-session");
    let (action, target) = intent.into_parts();
    assert_eq!(action.id(), "file.save");
    assert_eq!(target.document, 73);
}

#[test]
fn menu_intents_use_the_same_actions_without_keyboard_inference() {
    for action in FileAction::ALL {
        let intent = action.for_document(91_usize);
        assert_eq!(intent.action(), action);
        assert_eq!(intent.target_document(), &91);
    }
    let save_as = FileAction::SaveAs.for_document("opaque-document-A".to_owned());
    assert_eq!(
        save_as.into_parts(),
        (FileAction::SaveAs, "opaque-document-A".to_owned())
    );
    assert_eq!(PhysicalModifiers::default(), PhysicalModifiers::NONE);
}
