//! Faithful port of the state machine in the macOS version's
//! `event_handler.py` (class `HyperSpace`).
//!
//! Pure logic, no OS calls: `handle_key_event` returns whether the physical
//! event should pass through plus a list of synthetic key actions for the
//! caller to inject. This keeps the module unit-testable.

use std::collections::{HashMap, HashSet};

use crate::key_codes::{EXIT_KEY, VK_SPACE};

const VK_LWIN: u16 = 0x5B;
const VK_RWIN: u16 = 0x5C;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Idle,
    OnlySpaceDown,
    SpaceNormDown,
    HyperMode,
}

/// A target chord: main key plus modifiers to hold around it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Keys {
    pub main: u16,
    pub modifiers: Vec<u16>,
}

impl Keys {
    pub fn plain(main: u16) -> Self {
        Keys { main, modifiers: Vec::new() }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Full press (modifiers down, main down, main up, modifiers up).
    Press(Keys),
    /// Bare key-down only (used to replay the deferred candidate key).
    Down(u16),
    /// Quit the program (config target "exit").
    Exit,
}

pub struct Output {
    /// true = let the physical event through; false = suppress it.
    pub pass_through: bool,
    pub actions: Vec<Action>,
}

pub struct StateMachine {
    pub state: State,
    pub hold_as_hyper: bool,
    candidate_key: Option<u16>,
    map: HashMap<u16, Vec<Keys>>,
    pressed_modifiers: HashSet<u16>,
    /// Win keys currently held. Win always flows through to the OS (hooks
    /// cannot hide it from winlogon's raw-input Win+L match anyway; the
    /// DisableLockWorkstation policy handles that instead). Tracked so
    /// injected keys combine with the physically-held Win and so an
    /// intentional bare Win+L can be detected.
    os_wins: HashSet<u16>,
}

impl StateMachine {
    pub fn new(map: HashMap<u16, Vec<Keys>>, hold_as_hyper: bool) -> Self {
        StateMachine {
            state: State::Idle,
            hold_as_hyper,
            candidate_key: None,
            map,
            pressed_modifiers: HashSet::new(),
            os_wins: HashSet::new(),
        }
    }

    /// True if a Win key is held and the OS knows about it.
    pub fn os_win_held(&self) -> bool {
        !self.os_wins.is_empty()
    }

    /// Drop all held-key tracking and return to IDLE. Used around a lock,
    /// where key-up events never arrive (the session is switched away), so
    /// stale "still held" state would otherwise persist after unlock.
    pub fn clear_held_keys(&mut self) {
        self.state = State::Idle;
        self.candidate_key = None;
        self.pressed_modifiers.clear();
        self.os_wins.clear();
    }

    fn set_state(&mut self, state: State) {
        self.state = state;
    }

    /// Back to IDLE, dropping any deferred decision (used when the
    /// foreground app is blacklisted mid-sequence).
    pub fn reset(&mut self) {
        self.state = State::Idle;
        self.candidate_key = None;
    }

    /// Keep the modifier set fresh while events are being bypassed,
    /// so state is consistent when interception resumes.
    pub fn track_modifier(&mut self, key: u16, is_down: bool) {
        if is_down {
            self.pressed_modifiers.insert(key);
        } else {
            self.pressed_modifiers.remove(&key);
        }
        // Bypassed events reach the OS, so a bypassed Win is OS-visible.
        if key == VK_LWIN || key == VK_RWIN {
            if is_down {
                self.os_wins.insert(key);
            } else {
                self.os_wins.remove(&key);
            }
        }
    }

    /// Push a full press of `key`'s mapping (or the key itself if unmapped).
    /// Keys are injected plain; any physically-held modifier (incl. Win)
    /// combines with them at the OS level.
    fn press_mapped(&self, key: u16, actions: &mut Vec<Action>) {
        match self.map.get(&key) {
            Some(seq) => {
                for keys in seq {
                    if keys.main == EXIT_KEY {
                        actions.push(Action::Exit);
                    } else {
                        actions.push(Action::Press(keys.clone()));
                    }
                }
            }
            None => actions.push(Action::Press(Keys::plain(key))),
        }
    }

    fn press_plain(&self, vk: u16, actions: &mut Vec<Action>) {
        actions.push(Action::Press(Keys::plain(vk)));
    }

    pub fn handle_key_event(&mut self, key: u16, is_down: bool, is_modifier: bool) -> Output {
        let is_up = !is_down;
        let mut actions = Vec::new();

        if is_modifier {
            if is_down {
                self.pressed_modifiers.insert(key);
            } else {
                self.pressed_modifiers.remove(&key);
            }
        }

        // The Win key always flows through to the OS. Suppressing it never
        // actually stopped winlogon's raw-input Win+L (that is handled by
        // the DisableLockWorkstation policy), and suppressing the physical
        // Win-up left the modifier stuck down. We only track it and advance
        // the hyper state so space+Win+<key> injects into a held Win.
        if key == VK_LWIN || key == VK_RWIN {
            if is_down {
                self.os_wins.insert(key);
                if self.state == State::SpaceNormDown {
                    // Fire the deferred candidate before Win takes effect.
                    let cand = self.candidate_key.unwrap_or(0);
                    self.press_mapped(cand, &mut actions);
                    self.set_state(State::HyperMode);
                } else if self.state == State::OnlySpaceDown {
                    self.set_state(State::HyperMode);
                }
            } else {
                self.os_wins.remove(&key);
            }
            return Output { pass_through: true, actions };
        }

        let pass_through = match self.state {
            State::Idle => {
                // Hyper only engages when space is pressed first: with any
                // modifier already held, everything stays native.
                if !self.pressed_modifiers.is_empty() {
                    true
                } else if key == VK_SPACE && is_down {
                    self.set_state(State::OnlySpaceDown);
                    false
                } else {
                    true
                }
            }

            State::OnlySpaceDown => {
                // Space auto-repeat while held: with hold_as_hyper it means
                // "holding space alone enters hyper mode".
                if self.hold_as_hyper && key == VK_SPACE && is_down {
                    self.set_state(State::HyperMode);
                    false
                } else if key == VK_SPACE && is_up {
                    // A plain tap: replay the space press we swallowed.
                    self.set_state(State::Idle);
                    self.press_plain(VK_SPACE, &mut actions);
                    false
                } else if is_down {
                    if !is_modifier {
                        // Ambiguous: could be fast typing ("space then x")
                        // or a hyper chord. Defer the decision.
                        self.set_state(State::SpaceNormDown);
                        self.candidate_key = Some(key);
                        false
                    } else {
                        self.set_state(State::HyperMode);
                        true
                    }
                } else {
                    // A key-up of something pressed before space: normal typing.
                    self.set_state(State::Idle);
                    self.press_plain(VK_SPACE, &mut actions);
                    true
                }
            }

            State::SpaceNormDown => {
                let cand = self.candidate_key.unwrap_or(0);
                if key == VK_SPACE && is_up {
                    // Space released first: it was fast typing.
                    self.set_state(State::Idle);
                    self.press_plain(VK_SPACE, &mut actions);
                    actions.push(Action::Down(cand));
                    false
                } else if key == cand && is_up {
                    // Candidate released first: it was a hyper chord.
                    self.set_state(State::HyperMode);
                    self.press_mapped(cand, &mut actions);
                    false
                } else if key == cand && is_down {
                    // Candidate auto-repeat: hyper chord held down.
                    self.set_state(State::HyperMode);
                    self.press_mapped(cand, &mut actions);
                    self.press_mapped(cand, &mut actions);
                    false
                } else if is_down {
                    // A third key: hyper chord, fire both in order.
                    self.set_state(State::HyperMode);
                    self.press_mapped(cand, &mut actions);
                    self.press_mapped(key, &mut actions);
                    false
                } else {
                    // Key-up of something else: fall back to typing.
                    self.set_state(State::Idle);
                    self.press_plain(VK_SPACE, &mut actions);
                    actions.push(Action::Down(cand));
                    true
                }
            }

            State::HyperMode => {
                if key == VK_SPACE && is_up {
                    self.set_state(State::Idle);
                    false
                } else if key == VK_SPACE && is_down {
                    false
                } else if self.map.contains_key(&key) {
                    if is_down {
                        self.press_mapped(key, &mut actions);
                    }
                    // Both down and up of mapped keys are suppressed.
                    false
                } else {
                    true
                }
            }
        };

        Output { pass_through, actions }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key_codes::vk_from_name;

    const VK_H: u16 = b'H' as u16;
    const VK_J: u16 = b'J' as u16;
    const VK_C: u16 = b'C' as u16;
    const VK_Q: u16 = b'Q' as u16;
    const VK_LEFT: u16 = 0x25;
    const VK_DOWN_ARROW: u16 = 0x28;
    const VK_LSHIFT: u16 = 0xA0;
    const VK_CTRL: u16 = 0x11;

    fn test_map() -> HashMap<u16, Vec<Keys>> {
        let mut map = HashMap::new();
        map.insert(VK_H, vec![Keys::plain(VK_LEFT)]);
        map.insert(VK_J, vec![Keys::plain(VK_DOWN_ARROW)]);
        map.insert(VK_C, vec![Keys { main: VK_C, modifiers: vec![VK_CTRL] }]);
        map.insert(VK_Q, vec![Keys::plain(EXIT_KEY)]);
        map
    }

    fn sm() -> StateMachine {
        StateMachine::new(test_map(), true)
    }

    #[test]
    fn tapping_space_emits_one_space_press() {
        let mut m = sm();
        let o1 = m.handle_key_event(VK_SPACE, true, false);
        assert!(!o1.pass_through);
        assert!(o1.actions.is_empty());

        let o2 = m.handle_key_event(VK_SPACE, false, false);
        assert!(!o2.pass_through);
        assert_eq!(o2.actions, vec![Action::Press(Keys::plain(VK_SPACE))]);
        assert_eq!(m.state, State::Idle);
    }

    #[test]
    fn space_h_emits_left_arrow() {
        let mut m = sm();
        m.handle_key_event(VK_SPACE, true, false);

        let o_down = m.handle_key_event(VK_H, true, false);
        assert!(!o_down.pass_through);
        assert!(o_down.actions.is_empty(), "decision is deferred");
        assert_eq!(m.state, State::SpaceNormDown);

        let o_up = m.handle_key_event(VK_H, false, false);
        assert!(!o_up.pass_through);
        assert_eq!(o_up.actions, vec![Action::Press(Keys::plain(VK_LEFT))]);
        assert_eq!(m.state, State::HyperMode);
    }

    #[test]
    fn fast_typing_space_then_letter_replays_both() {
        // space down, h down, space up first => "space then h" typing.
        let mut m = sm();
        m.handle_key_event(VK_SPACE, true, false);
        m.handle_key_event(VK_H, true, false);

        let o = m.handle_key_event(VK_SPACE, false, false);
        assert!(!o.pass_through);
        assert_eq!(
            o.actions,
            vec![Action::Press(Keys::plain(VK_SPACE)), Action::Down(VK_H)]
        );
        assert_eq!(m.state, State::Idle);
    }

    #[test]
    fn candidate_repeat_fires_mapping_twice() {
        let mut m = sm();
        m.handle_key_event(VK_SPACE, true, false);
        m.handle_key_event(VK_H, true, false);

        let o = m.handle_key_event(VK_H, true, false); // auto-repeat
        assert!(!o.pass_through);
        assert_eq!(
            o.actions,
            vec![
                Action::Press(Keys::plain(VK_LEFT)),
                Action::Press(Keys::plain(VK_LEFT)),
            ]
        );
        assert_eq!(m.state, State::HyperMode);
    }

    #[test]
    fn third_key_fires_both_mappings_in_order() {
        let mut m = sm();
        m.handle_key_event(VK_SPACE, true, false);
        m.handle_key_event(VK_H, true, false);

        let o = m.handle_key_event(VK_J, true, false);
        assert!(!o.pass_through);
        assert_eq!(
            o.actions,
            vec![
                Action::Press(Keys::plain(VK_LEFT)),
                Action::Press(Keys::plain(VK_DOWN_ARROW)),
            ]
        );
        assert_eq!(m.state, State::HyperMode);
    }

    #[test]
    fn hold_repeat_enters_hyper_mode_without_actions() {
        let mut m = sm();
        m.handle_key_event(VK_SPACE, true, false);
        let o = m.handle_key_event(VK_SPACE, true, false); // auto-repeat
        assert!(!o.pass_through);
        assert!(o.actions.is_empty());
        assert_eq!(m.state, State::HyperMode);
    }

    #[test]
    fn mapped_key_up_is_suppressed_in_hyper_mode() {
        let mut m = sm();
        m.state = State::HyperMode;

        let o_down = m.handle_key_event(VK_H, true, false);
        assert!(!o_down.pass_through);
        assert_eq!(o_down.actions.len(), 1);

        let o_up = m.handle_key_event(VK_H, false, false);
        assert!(!o_up.pass_through);
        assert!(o_up.actions.is_empty());
    }

    #[test]
    fn unmapped_key_passes_through_in_hyper_mode() {
        let mut m = sm();
        m.state = State::HyperMode;
        let o = m.handle_key_event(b'Z' as u16, true, false);
        assert!(o.pass_through);
        assert!(o.actions.is_empty());
    }

    #[test]
    fn space_up_exits_hyper_mode_silently() {
        let mut m = sm();
        m.state = State::HyperMode;
        let o = m.handle_key_event(VK_SPACE, false, false);
        assert!(!o.pass_through);
        assert!(o.actions.is_empty());
        assert_eq!(m.state, State::Idle);
    }

    #[test]
    fn modifier_held_in_idle_passes_everything() {
        let mut m = sm();
        assert!(m.handle_key_event(VK_LSHIFT, true, true).pass_through);
        // Space while Shift held must not be intercepted.
        let o = m.handle_key_event(VK_SPACE, true, false);
        assert!(o.pass_through);
        assert_eq!(m.state, State::Idle);
        // Release Shift; space works as hyper again.
        m.handle_key_event(VK_LSHIFT, false, true);
        let o = m.handle_key_event(VK_SPACE, true, false);
        assert!(!o.pass_through);
        assert_eq!(m.state, State::OnlySpaceDown);
    }

    #[test]
    fn modifier_after_space_enters_hyper_and_passes() {
        let mut m = sm();
        m.handle_key_event(VK_SPACE, true, false);
        let o = m.handle_key_event(VK_LSHIFT, true, true);
        assert!(o.pass_through);
        assert_eq!(m.state, State::HyperMode);
    }

    #[test]
    fn space_then_win_flows_through_and_injects_plain_mapped_keys() {
        // Win passes through to the OS (no suppression); the held physical
        // Win combines with the plain injected arrow. Win-up must also pass
        // through so the modifier never gets stuck.
        let mut m = sm();
        assert!(!m.handle_key_event(VK_SPACE, true, false).pass_through);

        let o_win = m.handle_key_event(VK_LWIN, true, true);
        assert!(o_win.pass_through, "physical Win must reach the OS");
        assert!(m.os_win_held());
        assert_eq!(m.state, State::HyperMode);

        // h -> left_arrow, injected plain (OS-held Win makes it Win+Left).
        let o_h = m.handle_key_event(VK_H, true, false);
        assert!(!o_h.pass_through);
        assert_eq!(o_h.actions, vec![Action::Press(Keys::plain(VK_LEFT))]);

        // Win release passes through (no stuck modifier).
        let o_up = m.handle_key_event(VK_LWIN, false, true);
        assert!(o_up.pass_through);
        assert!(!m.os_win_held());
    }

    #[test]
    fn win_release_always_passes_through() {
        // Regression: the physical Win-up must never be swallowed, or the
        // OS keeps Win logically down (single A behaves like Win+A).
        let mut m = sm();
        m.handle_key_event(VK_SPACE, true, false);
        m.handle_key_event(VK_LWIN, true, true);
        m.handle_key_event(VK_H, true, false);
        m.handle_key_event(VK_H, false, false);
        let o_up = m.handle_key_event(VK_LWIN, false, true);
        assert!(o_up.pass_through);
        assert!(!m.os_win_held());
    }

    #[test]
    fn win_pressed_before_space_disables_hyper() {
        let mut m = sm();
        // Win down in IDLE stays native (Start menu, Win+L all work).
        assert!(m.handle_key_event(VK_LWIN, true, true).pass_through);

        // Hyper only engages when space comes first: everything passes.
        assert!(m.handle_key_event(VK_SPACE, true, false).pass_through);
        assert_eq!(m.state, State::Idle);
        assert!(m.handle_key_event(VK_H, true, false).pass_through);

        // The OS saw this Win go down, so its release must pass through.
        let o_up = m.handle_key_event(VK_LWIN, false, true);
        assert!(o_up.pass_through);
        assert_eq!(m.state, State::Idle);
    }

    #[test]
    fn win_tap_in_idle_stays_native() {
        let mut m = sm();
        assert!(m.handle_key_event(VK_LWIN, true, true).pass_through);
        assert!(m.os_win_held(), "Idle Win is OS-visible");
        assert!(m.handle_key_event(VK_LWIN, false, true).pass_through);
        assert!(!m.os_win_held());
        assert_eq!(m.state, State::Idle);
    }

    #[test]
    fn win_in_hyper_is_os_visible() {
        // Win flows through even inside a hyper flow, so it is OS-visible
        // and released cleanly.
        let mut m = sm();
        m.handle_key_event(VK_SPACE, true, false);
        m.handle_key_event(VK_LWIN, true, true);
        assert!(m.os_win_held());
        m.handle_key_event(VK_SPACE, false, false);
        assert_eq!(m.state, State::Idle);
        // Win still physically held until its own up arrives.
        assert!(m.os_win_held());
        assert!(m.handle_key_event(VK_LWIN, false, true).pass_through);
        assert!(!m.os_win_held());
    }

    #[test]
    fn clear_held_keys_resets_os_win_tracking() {
        // Reproduces the lock loop: after locking, key-ups are lost, so we
        // clear tracking; a subsequent bare L must not look like Win+L.
        let mut m = sm();
        m.handle_key_event(VK_LWIN, true, true);
        assert!(m.os_win_held());

        m.clear_held_keys();
        assert!(!m.os_win_held());
        assert_eq!(m.state, State::Idle);

        // Bare L in IDLE without Win held stays native.
        let o = m.handle_key_event(b'L' as u16, true, false);
        assert!(o.pass_through);
        assert!(o.actions.is_empty());
    }

    #[test]
    fn tracked_modifier_win_is_os_visible() {
        // Blacklist bypass path: Win reaches the OS, so it counts.
        let mut m = sm();
        m.track_modifier(VK_LWIN, true);
        assert!(m.os_win_held());
        m.track_modifier(VK_LWIN, false);
        assert!(!m.os_win_held());
    }

    #[test]
    fn exit_mapping_emits_exit_action() {
        let mut m = sm();
        m.handle_key_event(VK_SPACE, true, false);
        m.handle_key_event(VK_Q, true, false);
        let o = m.handle_key_event(VK_Q, false, false);
        assert_eq!(o.actions, vec![Action::Exit]);
    }

    #[test]
    fn mapping_with_modifiers_is_preserved() {
        let mut m = sm();
        m.handle_key_event(VK_SPACE, true, false);
        m.handle_key_event(VK_C, true, false);
        let o = m.handle_key_event(VK_C, false, false);
        assert_eq!(
            o.actions,
            vec![Action::Press(Keys { main: VK_C, modifiers: vec![VK_CTRL] })]
        );
    }

    #[test]
    fn key_name_resolution_matches_mac_semantics() {
        assert_eq!(vk_from_name("delete"), Some(0x08)); // mac delete = backspace
        assert_eq!(vk_from_name("forward_delete"), Some(0x2E));
        assert_eq!(vk_from_name("command"), vk_from_name("control"));
        assert_eq!(vk_from_name("option"), vk_from_name("alt"));
        assert_eq!(vk_from_name("k_1"), Some(0x31));
        assert_eq!(vk_from_name("h"), Some(b'H' as u16));
        assert_eq!(vk_from_name("nonexistent"), None);
    }
}
