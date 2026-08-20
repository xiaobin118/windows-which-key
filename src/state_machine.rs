use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, Instant};
use crate::types::*;
use crate::registry::ShortcutRegistry;

const DELAY_MS: u64 = 300;

#[derive(Debug, Clone, PartialEq)]
pub enum State {
    Idle,
    Waiting { deadline: Instant },
    Showing { path: Vec<ShortcutKey> },
}

pub struct StateMachine {
    pub state: State,
    pub pressed_modifiers: HashSet<Modifier>,
    registry: Arc<ShortcutRegistry>,
}

impl StateMachine {
    pub fn new(registry: Arc<ShortcutRegistry>) -> Self {
        StateMachine {
            state: State::Idle,
            pressed_modifiers: HashSet::new(),
            registry,
        }
    }

    pub fn handle_event(&mut self, event: KeyEvent) -> Option<UiCommand> {
        match event {
            KeyEvent::ModifierDown(modifier) => self.handle_modifier_down(modifier),
            KeyEvent::ModifierUp(modifier) => self.handle_modifier_up(modifier),
            KeyEvent::KeyDown(key) => self.handle_key_down(key),
            KeyEvent::KeyUp(_) => None, // Ignore key up events
        }
    }

    fn handle_modifier_down(&mut self, modifier: Modifier) -> Option<UiCommand> {
        self.pressed_modifiers.insert(modifier);

        match &self.state {
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

        // Only return to Idle when all modifiers are released
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
            State::Showing { path } => {
                // Try to resolve the key in the current path
                let shortcut_key = ShortcutKey {
                    modifiers: ModifierSet::empty(),
                    key,
                };

                match self.registry.resolve(path, shortcut_key.clone()) {
                    ResolveResult::Leaf(entry) => {
                        // Show leaf info (for MVP, we just log it)
                        log::info!("Leaf: {} - {}", entry.key, entry.desc);
                        None
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

    pub fn tick(&mut self) -> Option<UiCommand> {
        match &self.state {
            State::Waiting { deadline } => {
                if Instant::now() >= *deadline && !self.pressed_modifiers.is_empty() {
                    // Transition to Showing
                    self.state = State::Showing { path: vec![] };

                    let entries = self.registry.entries_at(&[]);
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
        root.children.insert(copy_key, Node::new(Some("Copy".to_string())));

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
        git_node.children.insert(status_key, Node::new(Some("Git status".to_string())));

        root.children.insert(git_key, git_node);

        Arc::new(ShortcutRegistry {
            globals: root,
            applications: HashMap::new(),
        })
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

        // Get to Showing state
        sm.handle_event(KeyEvent::ModifierDown(Modifier::Ctrl));
        sm.state = State::Showing { path: vec![] };

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
