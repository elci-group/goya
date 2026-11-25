//! Goya Pairs reducer.
//!
//! The deck arrangement is portable state. A second device reconstructs the
//! same board from the snapshot; it does not shuffle again. Matching two cards
//! and scoring that match are one transaction. Derived completion is computed
//! from the score and is not stored.

use crate::canonical::Value;
use crate::mutation::{Envelope, Operation};
use crate::schema::{Class, FieldDef, Schema, WireType};
use crate::session::{Intent, Replica};
use crate::surface::Permissions;
use crate::Error;
use std::collections::BTreeMap;

pub const PAIRS_ID: &str = "goya.pairs";
pub const F_BOARD: u32 = 1;
pub const F_MATCHED: u32 = 2;
pub const F_SELECTED: u32 = 3;
pub const F_MOVES: u32 = 4;
pub const F_SCORE: u32 = 5;
pub const F_PHASE: u32 = 6;
pub const F_DIFFICULTY: u32 = 7;
pub const F_REVEAL: u32 = 8;
pub const F_VIEWPORT: u32 = 9;
pub const F_POINTER: u32 = 10;
pub const F_SECRET: u32 = 11;

const PHASE_PLAYING: u32 = 1;
const PHASE_REVEALING: u32 = 2;
const PHASE_COMPLETE: u32 = 3;
pub const ACTION_SELECT: u32 = 1;
pub const ACTION_RESOLVE: u32 = 2;
const DECK: usize = 8;
const PAIR_KEYS: usize = 4;

const FIELDS: [FieldDef; 11] = [
    FieldDef {
        id: F_BOARD,
        name: "board",
        class: Class::Shared,
        wire: WireType::IntList,
    },
    FieldDef {
        id: F_MATCHED,
        name: "matched",
        class: Class::Authoritative,
        wire: WireType::IntList,
    },
    FieldDef {
        id: F_SELECTED,
        name: "selected",
        class: Class::Shared,
        wire: WireType::IntList,
    },
    FieldDef {
        id: F_MOVES,
        name: "moves",
        class: Class::Authoritative,
        wire: WireType::Int,
    },
    FieldDef {
        id: F_SCORE,
        name: "score",
        class: Class::Authoritative,
        wire: WireType::Int,
    },
    FieldDef {
        id: F_PHASE,
        name: "phase",
        class: Class::Authoritative,
        wire: WireType::Enum,
    },
    FieldDef {
        id: F_DIFFICULTY,
        name: "difficulty",
        class: Class::Shared,
        wire: WireType::Enum,
    },
    FieldDef {
        id: F_REVEAL,
        name: "reveal_pending",
        class: Class::Resumable,
        wire: WireType::Int,
    },
    FieldDef {
        id: F_VIEWPORT,
        name: "viewport",
        class: Class::Local,
        wire: WireType::Int,
    },
    FieldDef {
        id: F_POINTER,
        name: "pointer",
        class: Class::Ephemeral,
        wire: WireType::Int,
    },
    FieldDef {
        id: F_SECRET,
        name: "secret",
        class: Class::Private,
        wire: WireType::Text,
    },
];

pub const SCHEMA: Schema = Schema {
    application_id: PAIRS_ID,
    version: 1,
    fields: &FIELDS,
};

struct View {
    board: Vec<u32>,
    matched: Vec<u32>,
    selected: Vec<u32>,
    moves: i64,
    score: i64,
    phase: u32,
    reveal: i64,
}

pub fn open(me: [u8; 16], session: [u8; 16], arrangement: &[u32]) -> Result<Replica, Error> {
    if !arrangement_ok(arrangement) {
        return Err(Error::new(
            "RMT-E1001",
            "board arrangement is not a fixed pairs deck",
        ));
    }
    let mut values = BTreeMap::new();
    values.insert(F_BOARD, int_list(arrangement));
    values.insert(F_MATCHED, int_list(&[]));
    values.insert(F_SELECTED, int_list(&[]));
    values.insert(F_MOVES, Value::Int(0));
    values.insert(F_SCORE, Value::Int(0));
    values.insert(F_PHASE, Value::Enum(PHASE_PLAYING));
    values.insert(F_DIFFICULTY, Value::Enum(1));
    values.insert(F_REVEAL, Value::Int(0));
    Replica::open(SCHEMA, me, session, values)
}

pub fn join(me: [u8; 16], snapshot: &[u8]) -> Result<Replica, Error> {
    Replica::join(SCHEMA, me, Permissions::follower(), snapshot)
}

pub fn resume(me: [u8; 16], snapshot: &[u8]) -> Result<Replica, Error> {
    let probe = Replica::join(SCHEMA, me, Permissions::reader(), snapshot)?;
    let permissions = if probe.authority() == me {
        Permissions::authority()
    } else {
        Permissions::follower()
    };
    Replica::join(SCHEMA, me, permissions, snapshot)
}

pub fn select(replica: &mut Replica, card: u32) -> Result<Envelope, Error> {
    let view = require_view(replica)?;
    if view.phase != PHASE_PLAYING || card as usize >= view.board.len() {
        return Err(Error::new("RMT-E5004", "card selection is not legal"));
    }
    if view.matched.contains(&card) || view.selected.contains(&card) {
        return Err(Error::new("RMT-E5004", "card selection is not legal"));
    }
    let mut selected = view.selected.clone();
    selected.push(card);
    if selected.len() == 1 {
        return replica.commit_authority(
            vec![Operation::Set {
                field: F_SELECTED,
                value: int_list(&selected),
            }],
            state_ok,
        );
    }
    if selected.len() != 2 {
        return Err(Error::new("RMT-E5004", "card selection is not legal"));
    }
    let moves = view
        .moves
        .checked_add(1)
        .ok_or(Error::new("RMT-E8001", "move counter would overflow"))?;
    let left = selected[0] as usize;
    let right = selected[1] as usize;
    if view.board[left] == view.board[right] {
        let mut matched = view.matched.clone();
        matched.push(selected[0]);
        matched.push(selected[1]);
        matched.sort_unstable();
        let score = view
            .score
            .checked_add(1)
            .ok_or(Error::new("RMT-E8001", "score counter would overflow"))?;
        let phase = if score == (view.board.len() as i64) / 2 {
            PHASE_COMPLETE
        } else {
            PHASE_PLAYING
        };
        replica.commit_authority(
            vec![
                Operation::Set {
                    field: F_SELECTED,
                    value: int_list(&[]),
                },
                Operation::Set {
                    field: F_MATCHED,
                    value: int_list(&matched),
                },
                Operation::Set {
                    field: F_SCORE,
                    value: Value::Int(score),
                },
                Operation::Set {
                    field: F_MOVES,
                    value: Value::Int(moves),
                },
                Operation::Set {
                    field: F_PHASE,
                    value: Value::Enum(phase),
                },
                Operation::Set {
                    field: F_REVEAL,
                    value: Value::Int(0),
                },
            ],
            state_ok,
        )
    } else {
        replica.commit_authority(
            vec![
                Operation::Set {
                    field: F_SELECTED,
                    value: int_list(&selected),
                },
                Operation::Set {
                    field: F_MOVES,
                    value: Value::Int(moves),
                },
                Operation::Set {
                    field: F_PHASE,
                    value: Value::Enum(PHASE_REVEALING),
                },
                Operation::Set {
                    field: F_REVEAL,
                    value: Value::Int(1),
                },
            ],
            state_ok,
        )
    }
}

pub fn resolve_reveal(replica: &mut Replica) -> Result<Envelope, Error> {
    let view = require_view(replica)?;
    if view.phase != PHASE_REVEALING {
        return Err(Error::new("RMT-E5004", "no card reveal is pending"));
    }
    replica.commit_authority(
        vec![
            Operation::Set {
                field: F_SELECTED,
                value: int_list(&[]),
            },
            Operation::Set {
                field: F_PHASE,
                value: Value::Enum(PHASE_PLAYING),
            },
            Operation::Set {
                field: F_REVEAL,
                value: Value::Int(0),
            },
        ],
        state_ok,
    )
}

pub fn enqueue_select(replica: &mut Replica, card: u32) -> Result<(), Error> {
    replica.enqueue_intent(Intent {
        action: ACTION_SELECT,
        argument: u64::from(card),
        origin: replica.me(),
    })
}

pub fn execute_intent(replica: &mut Replica, intent: Intent) -> Result<Envelope, Error> {
    match intent.action {
        ACTION_SELECT => {
            let card = u32::try_from(intent.argument).map_err(|_| {
                Error::new(
                    "RMT-E1001",
                    "pairs intent argument does not fit a card index",
                )
            })?;
            select(replica, card)
        }
        ACTION_RESOLVE => resolve_reveal(replica),
        _ => Err(Error::new("RMT-E1001", "unknown pairs intent")),
    }
}

pub fn remaining_pairs(replica: &Replica) -> Result<u64, Error> {
    let view = require_view(replica)?;
    Ok((view.board.len() as u64 / 2).saturating_sub(view.score as u64))
}

pub fn completion_permille(replica: &Replica) -> Result<u64, Error> {
    let view = require_view(replica)?;
    let pairs = (view.board.len() as i64) / 2;
    let scaled = view
        .score
        .checked_mul(1000)
        .and_then(|value| value.checked_div(pairs))
        .ok_or(Error::new("RMT-E2001", "completion could not be derived"))?;
    u64::try_from(scaled).map_err(|_| Error::new("RMT-E2001", "completion could not be derived"))
}

pub(crate) fn state_ok(values: &BTreeMap<u32, Value>) -> bool {
    let Some(view) = view(values) else {
        return false;
    };
    legal(&view) && matches!(values.get(&F_DIFFICULTY), Some(Value::Enum(1)))
}

fn require_view(replica: &Replica) -> Result<View, Error> {
    view(replica.values()).ok_or(Error::new(
        "RMT-E2001",
        "replica state is not a legal pairs session",
    ))
}

fn view(values: &BTreeMap<u32, Value>) -> Option<View> {
    Some(View {
        board: list_u32(values.get(&F_BOARD)?)?,
        matched: list_u32(values.get(&F_MATCHED)?)?,
        selected: list_u32(values.get(&F_SELECTED)?)?,
        moves: integer(values.get(&F_MOVES)?)?,
        score: integer(values.get(&F_SCORE)?)?,
        phase: enumeration(values.get(&F_PHASE)?)?,
        reveal: integer(values.get(&F_REVEAL)?)?,
    })
}

fn legal(view: &View) -> bool {
    if !arrangement_ok(&view.board) || view.moves < 0 || view.score < 0 {
        return false;
    }
    if !strictly_increasing(&view.matched) || !unique_cards(&view.selected, view.board.len()) {
        return false;
    }
    if view
        .matched
        .iter()
        .any(|card| *card as usize >= view.board.len())
    {
        return false;
    }
    if view.selected.iter().any(|card| view.matched.contains(card)) {
        return false;
    }
    if !whole_pairs(&view.board, &view.matched, view.score) {
        return false;
    }
    let complete = view.score == (view.board.len() as i64) / 2;
    let minimum_moves = if view.phase == PHASE_REVEALING {
        view.score + 1
    } else {
        view.score
    };
    if view.moves < minimum_moves {
        return false;
    }
    match view.phase {
        PHASE_PLAYING => !complete && view.reveal == 0 && view.selected.len() <= 1,
        PHASE_REVEALING => {
            !complete
                && view.reveal == 1
                && view.selected.len() == 2
                && view.board[view.selected[0] as usize] != view.board[view.selected[1] as usize]
        }
        PHASE_COMPLETE => complete && view.reveal == 0 && view.selected.is_empty(),
        _ => false,
    }
}

fn whole_pairs(board: &[u32], matched: &[u32], score: i64) -> bool {
    let mut whole = 0i64;
    for key in 0..PAIR_KEYS {
        let mut members = Vec::new();
        for (index, pair) in board.iter().enumerate() {
            if *pair == key as u32 {
                members.push(index as u32);
            }
        }
        if members.len() != 2 {
            return false;
        }
        let left = matched.contains(&members[0]);
        let right = matched.contains(&members[1]);
        if left != right {
            return false;
        }
        if left {
            whole += 1;
        }
    }
    whole == score
}

fn arrangement_ok(arrangement: &[u32]) -> bool {
    if arrangement.len() != DECK {
        return false;
    }
    let mut counts = [0u8; PAIR_KEYS];
    for pair in arrangement {
        if *pair as usize >= PAIR_KEYS {
            return false;
        }
        counts[*pair as usize] = counts[*pair as usize].saturating_add(1);
    }
    counts == [2, 2, 2, 2]
}

fn strictly_increasing(values: &[u32]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}

fn unique_cards(values: &[u32], deck: usize) -> bool {
    let mut seen = [false; DECK];
    for card in values {
        let index = *card as usize;
        if index >= deck || index >= seen.len() || seen[index] {
            return false;
        }
        seen[index] = true;
    }
    true
}

fn int_list(values: &[u32]) -> Value {
    Value::Array(
        values
            .iter()
            .copied()
            .map(|value| Value::Int(i64::from(value)))
            .collect(),
    )
}

fn list_u32(value: &Value) -> Option<Vec<u32>> {
    let Value::Array(items) = value else {
        return None;
    };
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        out.push(u32::try_from(integer(item)?).ok()?);
    }
    Some(out)
}

fn integer(value: &Value) -> Option<i64> {
    match value {
        Value::Int(number) => Some(*number),
        _ => None,
    }
}

fn enumeration(value: &Value) -> Option<u32> {
    match value {
        Value::Enum(discriminant) => Some(*discriminant),
        _ => None,
    }
}
