use crate::registry::ShortcutRegistry;
use crate::types::*;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

const DELAY_MS: u64 = 300;

#[derive(Debug, Clone, PartialEq)]
pub enum State {
    Idle,
    Waiting { deadline: Instant },
    Showing { path: Vec<ShortcutKey> },
    BrowsingAll,
}

pub struct StateMachine {
    pub state: State,
    pub pressed_modifiers: HashSet<Modifier>,
    registry: Arc<ShortcutRegistry>,
    app_name: String,
}

impl StateMachine {
    pub fn new(registry: Arc<ShortcutRegistry>) -> Self {
        StateMachine {
            state: State::Idle,
            pressed_modifiers: HashSet::new(),
            registry,
            app_name: String::new(),
        }
    }

    pub fn replace_registry(&mut self, registry: Arc<ShortcutRegistry>, app_name: String) {
        self.registry = registry;
        self.app_name = app_name;
    }

    pub fn dismiss(&mut self) -> Option<UiCommand> {
        if matches!(self.state, State::Idle) {
            return None;
        }
        self.state = State::Idle;
        self.pressed_modifiers.clear();
        Some(UiCommand::Hide)
    }

    pub fn handle_event(&mut self, event: KeyEvent) -> Option<UiCommand> {
        match event {
            KeyEvent::ModifierDown(modifier) => self.handle_modifier_down(modifier),
            KeyEvent::ModifierUp(modifier) => self.handle_modifier_up(modifier),
            KeyEvent::KeyDown(key) => self.handle_key_down(key),
            KeyEvent::KeyUp(_) => None, // Ignore key up events
            KeyEvent::ToggleShowAll => self.handle_toggle_show_all(),
        }
    }

    fn handle_modifier_down(&mut self, modifier: Modifier) -> Option<UiCommand> {
        self.pressed_modifiers.insert(modifier);

        match &self.state {
            State::BrowsingAll => None,
            State::Idle => {
                // Start waiting
                let deadline = Instant::now() + Duration::from_millis(DELAY_MS);
                self.state = State::Waiting { deadline };
                None
            }
            State::Waiting { .. } | State::Showing { .. } => {
                // Already waiting or showing, just update modifier set
                None
            }
        }
    }

    fn handle_modifier_up(&mut self, modifier: Modifier) -> Option<UiCommand> {
        self.pressed_modifiers.remove(&modifier);

        if matches!(self.state, State::BrowsingAll) {
            return None;
        }

        if self.pressed_modifiers.is_empty() {
            self.state = State::Idle;
            Some(UiCommand::Hide)
        } else {
            None
        }
    }

    fn handle_key_down(&mut self, key: Key) -> Option<UiCommand> {
        match &self.state {
            State::Idle | State::Waiting { .. } => {
                // Not showing, ignore normal keys
                None
            }
            State::BrowsingAll => {
                if key.vk_code() == 0x1B {
                    self.state = State::Idle;
                    Some(UiCommand::Hide)
                } else {
                    None
                }
            }
            State::Showing { path } => {
                // Try to resolve the key in the current path
                let shortcut_key = ShortcutKey {
                    modifiers: self.current_modifiers(),
                    key,
                };

                match self.registry.resolve(path, shortcut_key.clone()) {
                    ResolveResult::Leaf(_) => {
                        self.state = State::Idle;
                        self.pressed_modifiers.clear();
                        Some(UiCommand::Hide)
                    }
                    ResolveResult::Group(breadcrumb) => {
                        // Navigate into group
                        let mut new_path = path.clone();
                        new_path.push(shortcut_key);

                        let entries = self.registry.entries_at(&new_path);
                        self.state = State::Showing { path: new_path };

                        Some(UiCommand::UpdateEntries {
                            entries,
                            breadcrumb,
                        })
                    }
                    ResolveResult::NotFound => {
                        // Unmatched key, hide and pass through
                        self.state = State::Idle;
                        self.pressed_modifiers.clear();
                        Some(UiCommand::Hide)
                    }
                }
            }
        }
    }

    fn handle_toggle_show_all(&mut self) -> Option<UiCommand> {
        if matches!(self.state, State::BrowsingAll) {
            self.state = State::Idle;
            return Some(UiCommand::Hide);
        }

        self.pressed_modifiers.clear();
        self.state = State::BrowsingAll;
        Some(UiCommand::ShowAll {
            app_name: self.app_name.clone(),
            entries: self.registry.all_entries(),
        })
    }

    fn current_modifiers(&self) -> ModifierSet {
        self.pressed_modifiers
            .iter()
            .fold(ModifierSet::empty(), |modifiers, modifier| {
                modifiers | ModifierSet::from_modifier(*modifier)
            })
    }

    #[cfg(test)]
    fn show_immediately_for_test(&mut self, modifiers: ModifierSet) {
        self.pressed_modifiers.clear();
        for modifier in [
            Modifier::Ctrl,
            Modifier::Alt,
            Modifier::Shift,
            Modifier::Meta,
        ] {
            if modifiers.contains_modifier(modifier) {
                self.pressed_modifiers.insert(modifier);
            }
        }
        self.state = State::Showing { path: vec![] };
    }

    pub fn tick(&mut self) -> Option<UiCommand> {
        match &self.state {
            State::Waiting { deadline } => {
                if Instant::now() >= *deadline && !self.pressed_modifiers.is_empty() {
                    // Transition to Showing
                    self.state = State::Showing { path: vec![] };

                    let entries = self
                        .registry
                        .root_entries_for_modifiers(self.current_modifiers());
                    let breadcrumb = vec![];

                    Some(UiCommand::Show {
                        position: (100, 100), // TODO: Calculate position based on cursor
                        entries,
                        breadcrumb,
                    })
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn build_test_registry() -> Arc<ShortcutRegistry> {
        let mut root = Node::new(None);

        let copy_key = ShortcutKey {
            modifiers: ModifierSet::CTRL,
            key: Key::C,
        };
        root.children
            .insert(copy_key, Node::new(Some("Copy".to_string())));

        root.children.insert(
            ShortcutKey {
                modifiers: ModifierSet::CTRL | ModifierSet::SHIFT,
                key: Key::P,
            },
            Node::new(Some("Command palette".to_string())),
        );

        let git_key = ShortcutKey {
            modifiers: ModifierSet::empty(),
            key: Key::G,
        };
        let mut git_node = Node::new(Some("Git".to_string()));
        git_node.group_name = Some("git".to_string());

        let status_key = ShortcutKey {
            modifiers: ModifierSet::empty(),
            key: Key::S,
        };
        git_node
            .children
            .insert(status_key, Node::new(Some("Git status".to_string())));

        root.children.insert(git_key, git_node);

        Arc::new(ShortcutRegistry {
            globals: root,
            applications: HashMap::new(),
        })
    }

    fn fixture_state_machine() -> StateMachine {
        StateMachine::new(build_test_registry())
    }

    fn sequence_state_machine() -> StateMachine {
        let mut root = Node::new(None);
        let mut prefix = Node::new(Some("Prefix".to_string()));
        prefix.children.insert(
            ShortcutKey {
                modifiers: ModifierSet::CTRL,
                key: Key::F,
            },
            Node::new(Some("Complete sequence".to_string())),
        );
        root.children.insert(
            ShortcutKey {
                modifiers: ModifierSet::CTRL,
                key: Key::K,
            },
            prefix,
        );
        StateMachine::new(Arc::new(ShortcutRegistry {
            globals: root,
            applications: HashMap::new(),
        }))
    }

    #[test]
    fn toggle_show_all_opens_and_closes_browser() {
        let mut sm = fixture_state_machine();

        let show = sm.handle_event(KeyEvent::ToggleShowAll);
        assert!(matches!(show, Some(UiCommand::ShowAll { .. })));
        assert_eq!(sm.state, State::BrowsingAll);

        let hide = sm.handle_event(KeyEvent::ToggleShowAll);
        assert!(matches!(hide, Some(UiCommand::Hide)));
        assert_eq!(sm.state, State::Idle);
    }

    #[test]
    fn show_all_uses_the_replaced_registry_and_application_name() {
        let mut sm = fixture_state_machine();
        let replacement = sequence_state_machine().registry;
        sm.replace_registry(replacement, "editor.exe".to_string());

        match sm.handle_event(KeyEvent::ToggleShowAll) {
            Some(UiCommand::ShowAll { app_name, entries }) => {
                assert_eq!(app_name, "editor.exe");
                assert_eq!(entries.len(), 1);
                assert_eq!(entries[0].key, "C-k, C-f");
            }
            _ => panic!("expected show-all command"),
        }
    }

    #[test]
    fn sequence_leaf_hides_after_completion() {
        let mut sm = sequence_state_machine();
        sm.show_immediately_for_test(ModifierSet::CTRL);

        assert!(matches!(
            sm.handle_event(KeyEvent::KeyDown(Key::K)),
            Some(UiCommand::UpdateEntries { .. })
        ));
        assert!(matches!(
            sm.handle_event(KeyEvent::KeyDown(Key::F)),
            Some(UiCommand::Hide)
        ));
    }

    #[test]
    fn ctrl_sequence_resolves_while_ctrl_is_held() {
        let mut sm = sequence_state_machine();

        sm.handle_event(KeyEvent::ModifierDown(Modifier::Ctrl));
        if let State::Waiting { deadline } = &mut sm.state {
            *deadline = Instant::now() - Duration::from_millis(1);
        }
        assert!(matches!(sm.tick(), Some(UiCommand::Show { .. })));

        assert!(matches!(
            sm.handle_event(KeyEvent::KeyDown(Key::K)),
            Some(UiCommand::UpdateEntries { .. })
        ));
        assert!(matches!(
            sm.handle_event(KeyEvent::KeyDown(Key::F)),
            Some(UiCommand::Hide)
        ));
        assert!(matches!(
            sm.handle_event(KeyEvent::ModifierUp(Modifier::Ctrl)),
            Some(UiCommand::Hide)
        ));
    }

    #[test]
    fn showing_root_entries_match_exactly_held_modifiers() {
        let mut sm = fixture_state_machine();

        sm.handle_event(KeyEvent::ModifierDown(Modifier::Ctrl));
        if let State::Waiting { deadline } = &mut sm.state {
            *deadline = Instant::now() - Duration::from_millis(1);
        }
        match sm.tick() {
            Some(UiCommand::Show { entries, .. }) => {
                assert_eq!(entries.len(), 1);
                assert_eq!(entries[0].key, "C-c");
            }
            command => panic!("expected Ctrl-only entries, got {command:?}"),
        }

        sm.dismiss();
        sm.handle_event(KeyEvent::ModifierDown(Modifier::Ctrl));
        sm.handle_event(KeyEvent::ModifierDown(Modifier::Shift));
        if let State::Waiting { deadline } = &mut sm.state {
            *deadline = Instant::now() - Duration::from_millis(1);
        }
        match sm.tick() {
            Some(UiCommand::Show { entries, .. }) => {
                assert_eq!(entries.len(), 1);
                assert_eq!(entries[0].key, "C-S-p");
            }
            command => panic!("expected Ctrl+Shift entries, got {command:?}"),
        }

        sm.dismiss();
        sm.handle_event(KeyEvent::ModifierDown(Modifier::Alt));
        if let State::Waiting { deadline } = &mut sm.state {
            *deadline = Instant::now() - Duration::from_millis(1);
        }
        match sm.tick() {
            Some(UiCommand::Show { entries, .. }) => assert!(entries.is_empty()),
            command => panic!("expected no Alt-only entries, got {command:?}"),
        }
    }

    #[test]
    fn test_idle_to_waiting_on_modifier_down() {
        let registry = build_test_registry();
        let mut sm = StateMachine::new(registry);

        let result = sm.handle_event(KeyEvent::ModifierDown(Modifier::Ctrl));

        assert!(matches!(sm.state, State::Waiting { .. }));
        assert!(result.is_none());
    }

    #[test]
    fn test_waiting_to_showing_after_delay() {
        let registry = build_test_registry();
        let mut sm = StateMachine::new(registry);

        sm.handle_event(KeyEvent::ModifierDown(Modifier::Ctrl));

        // Simulate time passing
        if let State::Waiting { deadline } = &mut sm.state {
            *deadline = Instant::now() - Duration::from_millis(1);
        }

        let result = sm.tick();

        assert!(matches!(sm.state, State::Showing { .. }));
        assert!(matches!(result, Some(UiCommand::Show { .. })));
    }

    #[test]
    fn test_showing_to_idle_on_modifier_release() {
        let registry = build_test_registry();
        let mut sm = StateMachine::new(registry);

        // Get to Showing state
        sm.handle_event(KeyEvent::ModifierDown(Modifier::Ctrl));
        sm.state = State::Showing { path: vec![] };

        let result = sm.handle_event(KeyEvent::ModifierUp(Modifier::Ctrl));

        assert_eq!(sm.state, State::Idle);
        assert!(matches!(result, Some(UiCommand::Hide)));
    }

    #[test]
    fn test_multiple_modifiers() {
        let registry = build_test_registry();
        let mut sm = StateMachine::new(registry);

        sm.handle_event(KeyEvent::ModifierDown(Modifier::Ctrl));
        sm.state = State::Showing { path: vec![] };

        // Press another modifier
        sm.handle_event(KeyEvent::ModifierDown(Modifier::Shift));
        assert_eq!(sm.pressed_modifiers.len(), 2);

        // Release one modifier - should stay in Showing
        let result = sm.handle_event(KeyEvent::ModifierUp(Modifier::Shift));
        assert!(matches!(sm.state, State::Showing { .. }));
        assert!(result.is_none());

        // Release last modifier - should go to Idle
        let result = sm.handle_event(KeyEvent::ModifierUp(Modifier::Ctrl));
        assert_eq!(sm.state, State::Idle);
        assert!(matches!(result, Some(UiCommand::Hide)));
    }

    #[test]
    fn test_normal_keys_ignored_when_idle() {
        let registry = build_test_registry();
        let mut sm = StateMachine::new(registry);

        let result = sm.handle_event(KeyEvent::KeyDown(Key::A));

        assert_eq!(sm.state, State::Idle);
        assert!(result.is_none());
    }

    #[test]
    fn test_group_navigation() {
        let registry = build_test_registry();
        let mut sm = StateMachine::new(registry);

        sm.show_immediately_for_test(ModifierSet::empty());

        // Press 'g' to navigate to Git group
        let result = sm.handle_event(KeyEvent::KeyDown(Key::G));

        assert!(matches!(sm.state, State::Showing { path } if path.len() == 1));
        assert!(matches!(result, Some(UiCommand::UpdateEntries { .. })));
    }

    #[test]
    fn test_unmatched_key_hides() {
        let registry = build_test_registry();
        let mut sm = StateMachine::new(registry);

        sm.handle_event(KeyEvent::ModifierDown(Modifier::Ctrl));
        sm.state = State::Showing { path: vec![] };

        // Press unmatched key
        let result = sm.handle_event(KeyEvent::KeyDown(Key::Z));

        assert_eq!(sm.state, State::Idle);
        assert!(matches!(result, Some(UiCommand::Hide)));
    }
}
