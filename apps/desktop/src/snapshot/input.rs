//! The events a person's pointer and keyboard would send, built for GPUI's
//! own event path so hover, clicks and focus behave as they do on screen.

use gpui::{
    Keystroke, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels,
    PlatformInput, Point, ScrollDelta, ScrollWheelEvent, TouchPhase, point, px,
};

use super::script::{KeyInput, PointerAction};

/// How far apart a moving pointer's events are, in points, as a mouse
/// reports them at an easy pace.
const POINTS_PER_MOVE: f32 = 16.;
/// The most events one move sends, each followed by a frame.
const MOST_MOVES: usize = 24;

pub fn pointer_move(
    position: Point<Pixels>,
    pressed_button: Option<MouseButton>,
    modifiers: Modifiers,
) -> PlatformInput {
    PlatformInput::MouseMove(MouseMoveEvent {
        position,
        pressed_button,
        modifiers,
    })
}

/// The presses and releases of a click, a double click or a right click
/// at `position`. A move has none.
pub fn clicks(
    action: PointerAction,
    position: Point<Pixels>,
    modifiers: Modifiers,
) -> Vec<PlatformInput> {
    let (button, count) = match action {
        PointerAction::Move => return Vec::new(),
        PointerAction::Click => (MouseButton::Left, 1),
        PointerAction::DoubleClick => (MouseButton::Left, 2),
        PointerAction::RightClick => (MouseButton::Right, 1),
    };
    (1..=count)
        .flat_map(|click_count| {
            [
                press(button, position, modifiers, click_count),
                release(button, position, modifiers, click_count),
            ]
        })
        .collect()
}

pub fn press(
    button: MouseButton,
    position: Point<Pixels>,
    modifiers: Modifiers,
    click_count: usize,
) -> PlatformInput {
    PlatformInput::MouseDown(MouseDownEvent {
        button,
        position,
        modifiers,
        click_count,
        first_mouse: false,
    })
}

pub fn release(
    button: MouseButton,
    position: Point<Pixels>,
    modifiers: Modifiers,
    click_count: usize,
) -> PlatformInput {
    PlatformInput::MouseUp(MouseUpEvent {
        button,
        position,
        modifiers,
        click_count,
    })
}

/// The pointer's path from `from` to `to`, ending at `to`, with
/// `pressed_button` held.
pub fn pointer_path(
    from: Point<Pixels>,
    to: Point<Pixels>,
    pressed_button: Option<MouseButton>,
    modifiers: Modifiers,
) -> Vec<PlatformInput> {
    let distance = f32::from((to.x - from.x).abs()).hypot(f32::from((to.y - from.y).abs()));
    let moves = ((distance / POINTS_PER_MOVE).ceil() as usize).clamp(1, MOST_MOVES);
    (1..=moves)
        .map(|step| {
            let along = step as f32 / moves as f32;
            let at = point(
                from.x + (to.x - from.x) * along,
                from.y + (to.y - from.y) * along,
            );
            pointer_move(at, pressed_button, modifiers)
        })
        .collect()
}

/// A wheel scroll at `position`; a positive `dy` scrolls toward the end.
pub fn scroll(position: Point<Pixels>, dy: f32) -> PlatformInput {
    PlatformInput::ScrollWheel(ScrollWheelEvent {
        position,
        delta: ScrollDelta::Pixels(point(px(0.), px(-dy))),
        modifiers: Modifiers::none(),
        touch_phase: TouchPhase::Moved,
    })
}

/// The keystrokes `keys` stands for: a key as the keymap writes it, or
/// a character at a time for text.
pub fn keystrokes(keys: &[KeyInput]) -> Result<Vec<Keystroke>, String> {
    let mut strokes = Vec::new();
    for key in keys {
        match key {
            KeyInput::Stroke(text) => strokes.push(
                Keystroke::parse(text).map_err(|error| format!("{text} isn't a key: {error}"))?,
            ),
            KeyInput::Text(text) => strokes.extend(text.chars().map(typed)),
        }
    }
    Ok(strokes)
}

/// The keystroke that types `character`.
fn typed(character: char) -> Keystroke {
    let key = match character {
        ' ' => "space".to_owned(),
        '\n' => "enter".to_owned(),
        '\t' => "tab".to_owned(),
        other => other.to_lowercase().collect(),
    };
    Keystroke {
        modifiers: Modifiers {
            shift: character.is_uppercase(),
            ..Modifiers::none()
        },
        key,
        key_char: Some(character.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_double_click_counts_its_clicks() {
        let events = clicks(
            PointerAction::DoubleClick,
            point(px(1.), px(2.)),
            Modifiers::none(),
        );
        let counts: Vec<usize> = events
            .iter()
            .map(|event| match event {
                PlatformInput::MouseDown(down) => down.click_count,
                PlatformInput::MouseUp(up) => up.click_count,
                _ => 0,
            })
            .collect();
        assert_eq!(counts, [1, 1, 2, 2]);
        assert!(
            clicks(
                PointerAction::Move,
                point(px(0.), px(0.)),
                Modifiers::none()
            )
            .is_empty()
        );
    }

    #[test]
    fn text_is_typed_a_character_at_a_time() {
        let strokes = keystrokes(&[
            KeyInput::Stroke("cmd-shift-p".into()),
            KeyInput::Text("Hi ".into()),
        ])
        .unwrap();
        assert_eq!(strokes.len(), 4);
        assert!(strokes[0].modifiers.platform && strokes[0].modifiers.shift);
        assert_eq!(strokes[1].key, "h");
        assert!(strokes[1].modifiers.shift);
        assert_eq!(strokes[1].key_char.as_deref(), Some("H"));
        assert_eq!(strokes[3].key, "space");
        let enter = keystrokes(&[KeyInput::Stroke("enter".into())]).unwrap();
        assert_eq!(enter[0].key, "enter");
    }

    #[test]
    fn a_drag_ends_where_it_was_asked_to() {
        let path = pointer_path(
            point(px(0.), px(0.)),
            point(px(120.), px(60.)),
            Some(MouseButton::Left),
            Modifiers::none(),
        );
        assert_eq!(path.len(), 9);
        let Some(PlatformInput::MouseMove(last)) = path.last() else {
            panic!("expected moves");
        };
        assert_eq!(last.position, point(px(120.), px(60.)));
        assert_eq!(last.pressed_button, Some(MouseButton::Left));
    }
}
