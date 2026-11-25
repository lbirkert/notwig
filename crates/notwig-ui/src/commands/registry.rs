use std::collections::BTreeMap;

use notwig_ext_api::{CommandDescriptor, validate_when};

use super::catalog::builtin_descriptors;

/// Built-in plus extension command descriptors resolved for this session.
#[derive(Debug, Clone, Default)]
pub struct CommandRegistry {
    descriptors: BTreeMap<String, CommandDescriptor>,
}

impl CommandRegistry {
    /// Creates a registry containing the built-in commands.
    #[must_use]
    pub fn new() -> Self {
        let mut registry = Self::default();
        registry.replace_extension_commands(Vec::new());
        registry
    }

    /// Replaces extension commands while keeping built-ins authoritative.
    ///
    /// Invalid conditions and duplicate ids are skipped. Sorting by stable ids
    /// makes the result independent of extension discovery order.
    pub fn replace_extension_commands(&mut self, commands: Vec<CommandDescriptor>) {
        self.descriptors.clear();
        for descriptor in builtin_descriptors() {
            self.descriptors.insert(descriptor.id.clone(), descriptor);
        }
        let mut extension_commands = commands;
        extension_commands.sort_by(|a, b| a.id.cmp(&b.id));
        for descriptor in extension_commands {
            if self.descriptors.contains_key(&descriptor.id) {
                continue;
            }
            if descriptor
                .when
                .as_deref()
                .is_some_and(|when| validate_when(when).is_err())
            {
                continue;
            }
            self.descriptors.insert(descriptor.id.clone(), descriptor);
        }
    }

    /// Returns a command by id.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&CommandDescriptor> {
        self.descriptors.get(id)
    }

    /// Returns all commands sorted by category, title, and id.
    #[must_use]
    pub fn sorted(&self) -> Vec<&CommandDescriptor> {
        let mut commands: Vec<_> = self.descriptors.values().collect();
        commands.sort_by(|a, b| {
            a.category
                .cmp(&b.category)
                .then_with(|| a.title.cmp(&b.title))
                .then_with(|| a.id.cmp(&b.id))
        });
        commands
    }

    /// Returns the extension-owned descriptors.
    #[must_use]
    pub fn extension_commands(&self) -> Vec<CommandDescriptor> {
        self.descriptors
            .values()
            .filter(|descriptor| descriptor.extension_id.is_some())
            .cloned()
            .collect()
    }

    /// Returns whether any command is owned by an extension.
    #[must_use]
    pub fn has_extension_commands(&self) -> bool {
        self.descriptors
            .values()
            .any(|descriptor| descriptor.extension_id.is_some())
    }
}
