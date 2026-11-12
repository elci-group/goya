//! goya-chess — local two-player chess exemplar.
//!
//! The board is declared in `apps/chess.gscene`. This host owns the rules,
//! overrides each square's text, and opens a window with `--live`.
//! Castling and en passant are omitted.
//!
//!   cargo run -p goya-hello --bin chess -- --live
//!
//! Without `--live`, the headless acceptance sequence runs.

use goya_lang::{layout, SceneStore};
use goya_runtime::Primitive;

const SCENE_PATH: &str = "apps/chess.gscene";
const EXTENT_W_PT: f64 = 100.0;
const EXTENT_H_PT: f64 = 112.0;
const WIN_W_PX: i32 = 400;
const WIN_H_PX: i32 = 448;
const ORIGIN_X: i64 = 6;
const ORIGIN_Y: i64 = 16;
const SQUARE: i64 = 11;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Side {
    Light,
    Night,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Man,
    Knight,
    Mitre,
    Rook,
    Lady,
    Ruler,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Piece {
    side: Side,
    kind: Kind,
}

struct Game {
    board: [Option<Piece>; 64],
    turn: Side,
    selected: Option<usize>,
    status: String,
}

impl Game {
    fn new() -> Self {
        let mut board = [None; 64];
        let back = [
            Kind::Rook,
            Kind::Knight,
            Kind::Mitre,
            Kind::Lady,
            Kind::Ruler,
            Kind::Mitre,
            Kind::Knight,
            Kind::Rook,
        ];
        for file in 0..8 {
            board[file] = Some(Piece {
                side: Side::Light,
                kind: back[file],
            });
            board[8 + file] = Some(Piece {
                side: Side::Light,
                kind: Kind::Man,
            });
            board[48 + file] = Some(Piece {
                side: Side::Night,
                kind: Kind::Man,
            });
            board[56 + file] = Some(Piece {
                side: Side::Night,
                kind: back[file],
            });
        }
        Game {
            board,
            turn: Side::Light,
            selected: None,
            status: "light".into(),
        }
    }

    fn empty() -> Self {
        Game {
            board: [None; 64],
            turn: Side::Light,
            selected: None,
            status: "light".into(),
        }
    }

    fn place(&mut self, index: usize, piece: Piece) {
        self.board[index] = Some(piece);
    }

    fn label_at(&self, index: usize) -> String {
        self.board[index].map(label).unwrap_or_default()
    }

    fn click(&mut self, index: usize) {
        if let Some(selected) = self.selected {
            if selected == index {
                self.selected = None;
                return;
            }
            if self.try_move(selected, index) {
                self.selected = None;
                return;
            }
        }
        if self.board[index].is_some_and(|piece| piece.side == self.turn) {
            self.selected = Some(index);
        }
    }

    fn try_move(&mut self, from: usize, to: usize) -> bool {
        let Some(piece) = self.board[from] else {
            self.status = "no".into();
            return false;
        };
        if piece.side != self.turn || !legal_destinations(&self.board, from).contains(&to) {
            self.status = "no".into();
            return false;
        }
        let captured = self.board[to].is_some();
        self.board[to] = Some(promote(piece, to));
        self.board[from] = None;
        self.turn = other(self.turn);
        self.status = if captured {
            "hit".into()
        } else {
            side_name(self.turn).into()
        };
        true
    }
}

fn other(side: Side) -> Side {
    match side {
        Side::Light => Side::Night,
        Side::Night => Side::Light,
    }
}

fn side_name(side: Side) -> &'static str {
    match side {
        Side::Light => "light",
        Side::Night => "night",
    }
}

fn label(piece: Piece) -> String {
    let name = match piece.kind {
        Kind::Man => "man",
        Kind::Knight => "n",
        Kind::Mitre => "mitre",
        Kind::Rook => "r",
        Kind::Lady => "lady",
        Kind::Ruler => "ruler",
    };
    name.to_string()
}

fn promote(piece: Piece, to: usize) -> Piece {
    let rank = rank_of(to);
    let last = match piece.side {
        Side::Light => 7,
        Side::Night => 0,
    };
    if piece.kind == Kind::Man && rank == last {
        Piece {
            side: piece.side,
            kind: Kind::Lady,
        }
    } else {
        piece
    }
}

fn file_of(index: usize) -> i32 {
    (index % 8) as i32
}

fn rank_of(index: usize) -> i32 {
    (index / 8) as i32
}

fn at(file: i32, rank: i32) -> Option<usize> {
    if (0..8).contains(&file) && (0..8).contains(&rank) {
        Some((rank * 8 + file) as usize)
    } else {
        None
    }
}

fn legal_destinations(board: &[Option<Piece>; 64], from: usize) -> Vec<usize> {
    let Some(piece) = board[from] else {
        return Vec::new();
    };
    pseudo(board, from, piece)
        .into_iter()
        .filter(|to| {
            let mut next = *board;
            next[*to] = Some(promote(piece, *to));
            next[from] = None;
            !attacked(&next, piece.side)
        })
        .collect()
}

fn pseudo(board: &[Option<Piece>; 64], from: usize, piece: Piece) -> Vec<usize> {
    let file = file_of(from);
    let rank = rank_of(from);
    let mut out = Vec::new();
    match piece.kind {
        Kind::Man => {
            let dir = if piece.side == Side::Light { 1 } else { -1 };
            let start = if piece.side == Side::Light { 1 } else { 6 };
            if let Some(step) = at(file, rank + dir) {
                if board[step].is_none() {
                    out.push(step);
                    if rank == start {
                        if let Some(two) = at(file, rank + 2 * dir) {
                            if board[two].is_none() {
                                out.push(two);
                            }
                        }
                    }
                }
            }
            for df in [-1, 1] {
                if let Some(hit) = at(file + df, rank + dir) {
                    if board[hit].is_some_and(|other| other.side != piece.side) {
                        out.push(hit);
                    }
                }
            }
        }
        Kind::Knight => {
            for (df, dr) in [
                (1, 2),
                (2, 1),
                (2, -1),
                (1, -2),
                (-1, -2),
                (-2, -1),
                (-2, 1),
                (-1, 2),
            ] {
                push_empty_or_enemy(board, piece.side, at(file + df, rank + dr), &mut out);
            }
        }
        Kind::Mitre => slide(
            board,
            piece.side,
            file,
            rank,
            &[(1, 1), (1, -1), (-1, 1), (-1, -1)],
            &mut out,
        ),
        Kind::Rook => slide(
            board,
            piece.side,
            file,
            rank,
            &[(1, 0), (-1, 0), (0, 1), (0, -1)],
            &mut out,
        ),
        Kind::Lady => slide(
            board,
            piece.side,
            file,
            rank,
            &[
                (1, 1),
                (1, -1),
                (-1, 1),
                (-1, -1),
                (1, 0),
                (-1, 0),
                (0, 1),
                (0, -1),
            ],
            &mut out,
        ),
        Kind::Ruler => {
            for df in -1..=1 {
                for dr in -1..=1 {
                    if df == 0 && dr == 0 {
                        continue;
                    }
                    push_empty_or_enemy(board, piece.side, at(file + df, rank + dr), &mut out);
                }
            }
        }
    }
    out
}

fn push_empty_or_enemy(
    board: &[Option<Piece>; 64],
    side: Side,
    index: Option<usize>,
    out: &mut Vec<usize>,
) {
    if let Some(index) = index {
        if !board[index].is_some_and(|piece| piece.side == side) {
            out.push(index);
        }
    }
}

fn slide(
    board: &[Option<Piece>; 64],
    side: Side,
    file: i32,
    rank: i32,
    dirs: &[(i32, i32)],
    out: &mut Vec<usize>,
) {
    for (df, dr) in dirs {
        let mut file = file + df;
        let mut rank = rank + dr;
        while let Some(index) = at(file, rank) {
            match board[index] {
                None => out.push(index),
                Some(piece) if piece.side != side => {
                    out.push(index);
                    break;
                }
                Some(_) => break,
            }
            file += df;
            rank += dr;
        }
    }
}

fn attacked(board: &[Option<Piece>; 64], side: Side) -> bool {
    let Some(ruler) = board.iter().position(|piece| {
        piece.is_some_and(|piece| piece.side == side && piece.kind == Kind::Ruler)
    }) else {
        return false;
    };
    let enemy = other(side);
    (0..64).any(|from| {
        board[from].is_some_and(|piece| piece.side == enemy) && attacks_square(board, from, ruler)
    })
}

fn attacks_square(board: &[Option<Piece>; 64], from: usize, target: usize) -> bool {
    let Some(piece) = board[from] else {
        return false;
    };
    if piece.kind == Kind::Man {
        let dir = if piece.side == Side::Light { 1 } else { -1 };
        let file = file_of(from);
        let rank = rank_of(from);
        return [-1, 1]
            .into_iter()
            .filter_map(|df| at(file + df, rank + dir))
            .any(|index| index == target);
    }
    pseudo(board, from, piece).contains(&target)
}

fn square_at(px: i32, py: i32) -> Option<usize> {
    let x = i64::from(px) / 4;
    let y = i64::from(py) / 4;
    if !(ORIGIN_X..ORIGIN_X + 8 * SQUARE).contains(&x)
        || !(ORIGIN_Y..ORIGIN_Y + 8 * SQUARE).contains(&y)
    {
        return None;
    }
    let file = (x - ORIGIN_X) / SQUARE;
    let from_top = (y - ORIGIN_Y) / SQUARE;
    let rank = 7 - from_top;
    at(file as i32, rank as i32)
}

fn with_state(mut nodes: Vec<goya_lang::DrawNode>, game: &Game) -> Vec<goya_lang::DrawNode> {
    for node in &mut nodes {
        if node.name == "status_text" {
            node.text = Some(game.status.clone());
        } else if let Some(index) = node
            .name
            .strip_prefix("sq_")
            .and_then(|rest| rest.parse().ok())
        {
            if index < 64 {
                node.text = Some(game.label_at(index));
                if game.selected == Some(index) {
                    node.fill = Some(0x00aa66);
                }
            }
        }
    }
    nodes
}

fn present_nodes(nodes: &[goya_lang::DrawNode]) -> Vec<Primitive> {
    nodes
        .iter()
        .map(|node| {
            let rect = goya_layout::Rect {
                x: node.rect.x,
                y: node.rect.y,
                width: node.rect.width,
                height: node.rect.height,
            };
            let label = node.text.clone().unwrap_or_default();
            if node.text.is_some() {
                Primitive::Text(rect, label)
            } else {
                Primitive::Panel(rect)
            }
        })
        .collect()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let live = args.iter().any(|arg| arg == "--live");
    let scene_path = args
        .iter()
        .position(|arg| arg == "--file")
        .and_then(|index| args.get(index + 1))
        .map(String::as_str)
        .unwrap_or(SCENE_PATH);
    let source = std::fs::read_to_string(scene_path).unwrap_or_else(|error| {
        eprintln!("goya-chess: cannot read {scene_path}: {error}");
        std::process::exit(1);
    });
    let mut store = match SceneStore::new(&source) {
        Ok(store) => store,
        Err(error) => {
            eprintln!("goya-chess: {error}");
            std::process::exit(1);
        }
    };
    if !live {
        self_test(&mut store);
        return;
    }
    let mut win = match goya_wayland::GoyaWindow::create("goya-chess", WIN_W_PX, WIN_H_PX) {
        Ok(win) => win,
        Err(error) => {
            eprintln!("goya-chess: live unavailable: {error}");
            return;
        }
    };
    let mut game = Game::new();
    let mut dirty = true;
    while !win.close_requested() {
        for event in win.poll() {
            if let goya_wayland::PlatformEvent::PointerPress(x, y) = event {
                if let Some(index) = square_at(x as i32, y as i32) {
                    game.click(index);
                    dirty = true;
                }
            } else if let goya_wayland::PlatformEvent::Resized(_, _) = event {
                dirty = true;
            }
        }
        if dirty {
            let nodes = with_state(layout(store.get(), EXTENT_W_PT, EXTENT_H_PT).nodes, &game);
            let fills: Vec<Option<u32>> = nodes.iter().map(|node| node.fill).collect();
            let prims = present_nodes(&nodes);
            let _ = win.present(&prims, &fills, &[], [235, 235, 240], [20, 20, 28], 3);
            dirty = false;
        }
        win.pump(1);
    }
}

fn self_test(store: &mut SceneStore) {
    let mut failures = Vec::new();
    let frame = layout(store.get(), EXTENT_W_PT, EXTENT_H_PT);
    for name in ["status_text", "sq_0", "sq_12", "sq_63"] {
        if frame.find(name).is_none() {
            failures.push(format!("missing {name}"));
        }
    }

    let mut game = Game::new();
    if !game.try_move(12, 28) {
        failures.push("man double step rejected".into());
    }
    let mut game = Game::new();
    if !game.try_move(1, 18) {
        failures.push("knight development rejected".into());
    }
    let mut game = Game::new();
    if game.try_move(12, 36) {
        failures.push("man triple step accepted".into());
    }
    if game.try_move(0, 1) {
        failures.push("friendly square accepted".into());
    }

    let mut game = Game::empty();
    game.place(
        28,
        Piece {
            side: Side::Light,
            kind: Kind::Man,
        },
    );
    game.place(
        35,
        Piece {
            side: Side::Night,
            kind: Kind::Man,
        },
    );
    if !game.try_move(28, 35) || game.board[35].is_none() {
        failures.push("capture rejected".into());
    }

    let mut game = Game::empty();
    game.place(
        4,
        Piece {
            side: Side::Light,
            kind: Kind::Ruler,
        },
    );
    game.place(
        0,
        Piece {
            side: Side::Night,
            kind: Kind::Rook,
        },
    );
    if game.try_move(4, 3) {
        failures.push("ruler stayed on an attacked rank".into());
    }
    if !game.try_move(4, 12) {
        failures.push("ruler step off the attacked rank rejected".into());
    }

    if failures.is_empty() {
        println!("goya-chess: ALL EXEMPLAR CHECKS PASS");
    } else {
        for failure in &failures {
            eprintln!("EXEMPLAR FAILURE: {failure}");
        }
        std::process::exit(1);
    }
}
