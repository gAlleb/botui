//! Optional aliases for physical navigation keys. Dispatch still belongs to the
//! active screen; editors and the Space command grammar receive raw input.
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    Reading,
    Pdf,
    Images,
    Vim,
    Library,
    Dialogs,
}

type Rules = BTreeMap<Scope, BTreeMap<String, String>>;
type Key = (KeyCode, KeyModifiers);
type Aliases = HashMap<(Scope, Key), Key>;

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(try_from = "Rules", into = "Rules")]
pub struct Keymap {
    rules: Rules,
    aliases: Aliases,
}

impl From<Keymap> for Rules {
    fn from(map: Keymap) -> Self {
        map.rules
    }
}
impl TryFrom<Rules> for Keymap {
    type Error = &'static str;
    fn try_from(rules: Rules) -> Result<Self, Self::Error> {
        let mut aliases = HashMap::new();
        for (&scope, section) in &rules {
            for (from, to) in section {
                let from = parse(from).ok_or("invalid alias key")?;
                let to = parse(to).ok_or("invalid destination key")?;
                if aliases.insert((scope, normalized(from)), to).is_some() {
                    return Err("duplicate aliases for the same key");
                }
            }
        }
        Ok(Self { rules, aliases })
    }
}

thread_local! { static ACTIVE: std::cell::RefCell<Aliases> = Default::default(); }
impl Keymap {
    pub fn activate(&self) {
        ACTIVE.with_borrow_mut(|active| active.clone_from(&self.aliases));
    }
    pub fn descriptions(&self) -> impl Iterator<Item = (String, String)> + '_ {
        self.rules.iter().flat_map(|(scope, rules)| {
            rules
                .iter()
                .map(move |(from, to)| (from.clone(), format!("{scope:?} · {to}")))
        })
    }
}

impl Scope {
    pub fn event(self, mut event: Event) -> Event {
        if let Event::Key(key) = &mut event {
            ACTIVE.with_borrow(|active| translate(active, self, key));
        }
        event
    }
}

fn translate(aliases: &Aliases, scope: Scope, key: &mut KeyEvent) {
    if let Some(&(code, modifiers)) = aliases.get(&(scope, normalized((key.code, key.modifiers)))) {
        key.code = code;
        key.modifiers = modifiers;
    }
}

fn normalized((code, mut mods): Key) -> Key {
    // Terminals vary in whether they report Shift on an already uppercase
    // character, and on BackTab (the traditional encoding of Shift+Tab).
    if matches!(code, KeyCode::Char(c) if c.is_uppercase()) || code == KeyCode::BackTab {
        mods.remove(KeyModifiers::SHIFT);
    }
    if code == KeyCode::Tab && mods.contains(KeyModifiers::SHIFT) {
        return (KeyCode::BackTab, mods - KeyModifiers::SHIFT);
    }
    (code, mods)
}

fn parse(mut name: &str) -> Option<Key> {
    let mut mods = KeyModifiers::empty();
    while let Some((prefix, rest)) = name.split_once('+') {
        let modifier = match prefix.to_ascii_lowercase().as_str() {
            "ctrl" => KeyModifiers::CONTROL,
            "alt" => KeyModifiers::ALT,
            "shift" => KeyModifiers::SHIFT,
            _ => break,
        };
        if mods.contains(modifier) {
            return None;
        }
        mods.insert(modifier);
        name = rest;
    }
    let mut chars = name.chars();
    let first = chars.next()?;
    let code = if chars.next().is_none() && !first.is_control() {
        if mods.contains(KeyModifiers::SHIFT) && first.is_ascii_alphabetic() {
            KeyCode::Char(first.to_ascii_uppercase())
        } else {
            KeyCode::Char(first)
        }
    } else {
        match name.to_ascii_lowercase().as_str() {
            "left" => KeyCode::Left,
            "right" => KeyCode::Right,
            "up" => KeyCode::Up,
            "down" => KeyCode::Down,
            "home" => KeyCode::Home,
            "end" => KeyCode::End,
            "pageup" | "pgup" => KeyCode::PageUp,
            "pagedown" | "pgdn" => KeyCode::PageDown,
            "enter" => KeyCode::Enter,
            "esc" | "escape" => KeyCode::Esc,
            "space" => KeyCode::Char(' '),
            "insert" => KeyCode::Insert,
            "delete" | "del" => KeyCode::Delete,
            "backspace" => KeyCode::Backspace,
            "tab" if mods.contains(KeyModifiers::SHIFT) => KeyCode::BackTab,
            "tab" => KeyCode::Tab,
            "backtab" => KeyCode::BackTab,
            f => KeyCode::F(
                f.strip_prefix('f')?
                    .parse()
                    .ok()
                    .filter(|n| (1..=24).contains(n))?,
            ),
        }
    };
    Some((code, mods))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEventKind, KeyEventState};

    #[test]
    fn aliases_are_scoped_single_step_and_keep_repeat_information() {
        let rules = BTreeMap::from([
            (
                Scope::Reading,
                BTreeMap::from([("x".into(), "Right".into()), ("Right".into(), "F3".into())]),
            ),
            (
                Scope::Vim,
                BTreeMap::from([
                    ("x".into(), "k".into()),
                    ("Shift+A".into(), "Ctrl+b".into()),
                ]),
            ),
        ]);
        let aliases = Keymap::try_from(rules).unwrap().aliases;
        let original = KeyEvent::new_with_kind_and_state(
            KeyCode::Char('x'),
            KeyModifiers::NONE,
            KeyEventKind::Repeat,
            KeyEventState::CAPS_LOCK,
        );
        for (scope, code) in [
            (Scope::Reading, KeyCode::Right),
            (Scope::Vim, KeyCode::Char('k')),
            (Scope::Library, KeyCode::Char('x')),
        ] {
            let mut key = original;
            translate(&aliases, scope, &mut key);
            assert_eq!(
                (key.code, key.kind, key.state),
                (code, original.kind, original.state)
            );
        }
        let mut shifted = KeyEvent::new(KeyCode::Char('A'), KeyModifiers::NONE);
        translate(&aliases, Scope::Vim, &mut shifted);
        assert_eq!((shifted.code, shifted.modifiers), parse("Ctrl+b").unwrap());
        assert_eq!(
            normalized(parse("Shift+Tab").unwrap()),
            normalized(parse("BackTab").unwrap())
        );
        assert_eq!(
            parse("Ctrl++"),
            Some((KeyCode::Char('+'), KeyModifiers::CONTROL))
        );
        for bad in ["", "Ctrl+", "foo", "F256", "Ctrl+Ctrl+x"] {
            assert!(parse(bad).is_none(), "{bad}");
        }
    }
}
