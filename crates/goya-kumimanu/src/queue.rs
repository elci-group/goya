use crate::error::Error;
use crate::Event;

/// Bound on queued events. Motion and expose are collapsed before a key is dropped.
pub const MAX_EVENTS: usize = 256;

/// Push `event` without letting the queue grow past [`MAX_EVENTS`].
///
/// Consecutive motion events collapse. Consecutive expose events collapse.
/// If the queue is full of events that must be kept, this returns
/// [`Error::EventQueueSaturated`] and leaves the queue unchanged.
pub fn push(queue: &mut Vec<Event>, event: Event) -> Result<(), Error> {
    if coalesce(queue, &event) {
        return Ok(());
    }
    if queue.len() >= MAX_EVENTS {
        if let Some(index) = queue.iter().position(Event::is_droppable) {
            queue.remove(index);
        } else {
            return Err(Error::EventQueueSaturated {
                capacity: MAX_EVENTS,
            });
        }
    }
    queue.push(event);
    Ok(())
}

fn coalesce(queue: &mut [Event], event: &Event) -> bool {
    let Some(last) = queue.last_mut() else {
        return false;
    };
    match (last, event) {
        (Event::PointerMove { x, y }, Event::PointerMove { x: nx, y: ny }) => {
            *x = *nx;
            *y = *ny;
            true
        }
        (Event::Exposed, Event::Exposed) => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Event;

    fn key(code: u8) -> Event {
        Event::Key {
            keycode: code,
            keysym: u32::from(code),
            text: None,
            named: None,
            modifiers: crate::Modifiers::default(),
            pressed: true,
            repeat: false,
        }
    }

    #[test]
    fn motion_collapses_to_the_latest_point() {
        let mut queue = Vec::new();
        push(&mut queue, Event::PointerMove { x: 1, y: 2 }).unwrap();
        push(&mut queue, Event::PointerMove { x: 3, y: 4 }).unwrap();
        assert_eq!(queue, vec![Event::PointerMove { x: 3, y: 4 }]);
    }

    #[test]
    fn a_full_queue_of_keys_is_rejected() {
        let mut queue = Vec::new();
        for code in 0..MAX_EVENTS {
            push(&mut queue, key(code as u8)).unwrap();
        }
        let err = push(&mut queue, key(9)).unwrap_err();
        assert_eq!(
            err,
            Error::EventQueueSaturated {
                capacity: MAX_EVENTS
            }
        );
        assert_eq!(queue.len(), MAX_EVENTS);
        assert!(matches!(queue[0], Event::Key { keycode: 0, .. }));
    }

    #[test]
    fn a_full_queue_drops_the_oldest_motion_before_a_key() {
        let mut queue = vec![Event::PointerMove { x: 1, y: 1 }];
        for code in 0..MAX_EVENTS - 1 {
            queue.push(key(code as u8));
        }
        push(&mut queue, key(7)).unwrap();
        assert_eq!(queue.len(), MAX_EVENTS);
        assert!(matches!(queue[0], Event::Key { .. }));
    }
}
