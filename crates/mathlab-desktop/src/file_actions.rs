// SPDX-License-Identifier: GPL-3.0-or-later
//! Pure preparation for MI01–MI03 and MI46; no desktop routing is installed.
//!
//! These descriptors represent documented platform candidates. The selected
//! MATLAB keyboard profile and the first interaction slice remain unqualified.
//! Callers own focus, eligibility, document lookup, dialogs, files and execution.

/// The platform whose documented candidate bindings are being displayed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Platform {
    MacOs,
    Windows,
    Linux,
}

/// Independent physical modifier categories, without a platform alias.
///
/// In particular, `command` must not be populated from egui's semantic
/// `Modifiers::command`, which also represents Control on Windows/Linux.
/// The adapter must supply independently observed physical flags.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct PhysicalModifiers {
    pub control: bool,
    pub command: bool,
    pub shift: bool,
    pub alt: bool,
}

impl PhysicalModifiers {
    pub const NONE: Self = Self {
        control: false,
        command: false,
        shift: false,
        alt: false,
    };
}

/// Delivered key categories needed by this model; no keyboard-layout inference.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FileKey {
    O,
    S,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BindingDescriptor {
    key: FileKey,
    modifiers: PhysicalModifiers,
}

impl BindingDescriptor {
    pub const fn key(self) -> FileKey {
        self.key
    }

    pub const fn modifiers(self) -> PhysicalModifiers {
        self.modifiers
    }

    /// Exact equality rejects every additional Shift/Alt/Control/Command flag.
    pub fn matches(self, key: FileKey, modifiers: PhysicalModifiers) -> bool {
        self.key == key && self.modifiers == modifiers
    }

    /// Render the same descriptor used for matching; no separate hint table.
    pub fn menu_hint(self) -> String {
        let mut parts = Vec::new();
        if self.modifiers.command {
            parts.push("Command");
        }
        if self.modifiers.control {
            parts.push("Ctrl");
        }
        if self.modifiers.alt {
            parts.push("Alt");
        }
        if self.modifiers.shift {
            parts.push("Shift");
        }
        parts.push(match self.key {
            FileKey::O => "O",
            FileKey::S => "S",
            FileKey::Other => unreachable!("file-action descriptors only contain O/S"),
        });
        parts.join("+")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FileAction {
    Open,
    Save,
    SaveAs,
}

impl FileAction {
    pub const ALL: [Self; 3] = [Self::Open, Self::Save, Self::SaveAs];

    /// Product action identities, independent of enum order or visible wording.
    pub const fn id(self) -> &'static str {
        match self {
            Self::Open => "file.open",
            Self::Save => "file.save",
            Self::SaveAs => "file.save_as",
        }
    }

    /// Documented candidates, not the observed installed MATLAB keymap.
    /// Save As is provisionally menu-only until its ordinary Editor row is known.
    pub const fn documented_binding(self, platform: Platform) -> Option<BindingDescriptor> {
        let key = match self {
            Self::Open => FileKey::O,
            Self::Save => FileKey::S,
            Self::SaveAs => return None,
        };
        let modifiers = match platform {
            Platform::MacOs => PhysicalModifiers {
                command: true,
                ..PhysicalModifiers::NONE
            },
            Platform::Windows | Platform::Linux => PhysicalModifiers {
                control: true,
                ..PhysicalModifiers::NONE
            },
        };
        Some(BindingDescriptor { key, modifiers })
    }

    pub fn menu_hint(self, platform: Platform) -> Option<String> {
        self.documented_binding(platform)
            .map(BindingDescriptor::menu_hint)
    }

    /// Bind an action to the caller's explicit identity, never an active index.
    pub fn for_document<Id>(self, target_document: Id) -> FileActionIntent<Id> {
        FileActionIntent {
            action: self,
            target_document,
        }
    }
}

/// An unexecuted request. The caller must resolve/revalidate its identity before
/// any effect; construction neither verifies existence nor chooses a target.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FileActionIntent<Id> {
    action: FileAction,
    target_document: Id,
}

impl<Id> FileActionIntent<Id> {
    pub const fn action(&self) -> FileAction {
        self.action
    }

    pub const fn target_document(&self) -> &Id {
        &self.target_document
    }

    pub fn into_parts(self) -> (FileAction, Id) {
        (self.action, self.target_document)
    }
}

/// Decode only an exact documented candidate chord, without deciding whether
/// its current focus/dialog/application state permits dispatch.
pub fn action_for_key(
    platform: Platform,
    key: FileKey,
    modifiers: PhysicalModifiers,
) -> Option<FileAction> {
    FileAction::ALL.into_iter().find(|action| {
        action
            .documented_binding(platform)
            .is_some_and(|binding| binding.matches(key, modifiers))
    })
}

pub fn intent_for_key<Id>(
    platform: Platform,
    key: FileKey,
    modifiers: PhysicalModifiers,
    target_document: Id,
) -> Option<FileActionIntent<Id>> {
    action_for_key(platform, key, modifiers).map(|action| action.for_document(target_document))
}
