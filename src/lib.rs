use rand::Rng;
use rcade_plugin_input_classic::ClassicController;
use std::cell::RefCell;
use std::rc::Rc;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement, KeyboardEvent};

const CANVAS_WIDTH: f64 = 330.0;
const CANVAS_HEIGHT: f64 = 250.0;
const PLAYER_WIDTH: f64 = 30.0;
const PLAYER_HEIGHT: f64 = 30.0;
const PLAYER_SPEED: f64 = 3.0;
const OBJECT_WIDTH: f64 = 20.0;
const OBJECT_HEIGHT: f64 = 20.0;

// Base falling speed for objects. This will be scaled by difficulty.
const OBJECT_SPEED: f64 = 3.0;

// Base spawn interval in "difficulty ticks". Real spawn rate speeds up as difficulty rises.
const BASE_SPAWN_INTERVAL: f64 = 45.0;

#[derive(Clone)]
enum ObjectType {
    GoodDeal, // Catch these for points
    BadItem,  // Dodge these or lose health
}

#[derive(Clone)]
struct FallingObject {
    x: f64,
    y: f64,
    obj_type: ObjectType,
}

struct Player {
    x: f64,
    y: f64,
}

struct PlayerSlot {
    player: Player,
    score: i32,
    health: i32,
    player_index: usize, // Original player slot (0 for P1, 1 for P2)
}

impl PlayerSlot {
    fn new(index: usize, total_players: usize) -> Self {
        let spacing = CANVAS_WIDTH / (total_players as f64 + 1.0);
        let target_center = spacing * (index as f64 + 1.0);
        PlayerSlot {
            player: Player {
                x: target_center - PLAYER_WIDTH / 2.0,
                y: CANVAS_HEIGHT - PLAYER_HEIGHT - 20.0,
            },
            score: 0,
            health: 3,
            player_index: index,
        }
    }
}

#[derive(Copy, Clone, PartialEq)]
enum PlayerMode {
    Single,
    Two,
}

impl PlayerMode {
    fn player_count(&self) -> usize {
        match self {
            PlayerMode::Single => 1,
            PlayerMode::Two => 2,
        }
    }
}

#[derive(PartialEq)]
enum GamePhase {
    ModeSelect,
    Playing,
    GameOver,
    NameEntry,
}

struct LeaderboardEntry {
    score: i32,
    mode: PlayerMode,
    name: String,
}

struct GameState {
    players: Vec<PlayerSlot>,
    objects: Vec<FallingObject>,
    frame_count: u32,
    difficulty_multiplier: f64,
    spawn_meter: f64,
    controller: Option<ClassicController>,
    mode: PlayerMode,
    phase: GamePhase,
    menu_selection: PlayerMode,
    last_system_one_player: bool,
    last_system_two_player: bool,
    last_confirm: bool,
    last_up: bool,
    last_down: bool,
    last_left: bool,
    last_right: bool,
    final_scores: Vec<(usize, i32)>, // (player_index, score) for dead players
    leaderboard: Vec<LeaderboardEntry>,
    pending_scores: Vec<(usize, i32)>, // Scores waiting for name entry
    current_name: String,
    name_entry_index: usize, // Which player we're entering name for
}

#[derive(Default, Clone)]
struct KeyboardState {
    system_one_player: bool,
    system_two_player: bool,
    player1_left: bool,
    player1_right: bool,
    player1_up: bool,
    player1_down: bool,
    player1_a: bool,
    player2_left: bool,
    player2_right: bool,
    player2_a: bool,
    last_key: Option<String>, // For name entry
}

impl KeyboardState {
    fn handle_code(&mut self, code: &str, pressed: bool) -> bool {
        match code {
            "Digit1" => {
                self.system_one_player = pressed;
                true
            }
            "Digit2" => {
                self.system_two_player = pressed;
                true
            }
            "ArrowLeft" => {
                self.player1_left = pressed;
                true
            }
            "ArrowRight" => {
                self.player1_right = pressed;
                true
            }
            "ArrowUp" => {
                self.player1_up = pressed;
                true
            }
            "ArrowDown" => {
                self.player1_down = pressed;
                true
            }
            "ControlLeft" => {
                self.player1_a = pressed;
                true
            }
            "KeyD" => {
                self.player2_left = pressed;
                true
            }
            "KeyG" => {
                self.player2_right = pressed;
                true
            }
            "KeyA" => {
                self.player2_a = pressed;
                true
            }
            _ => false,
        }
    }
}

#[derive(Default, Clone)]
struct InputSnapshot {
    system_one_player: bool,
    system_two_player: bool,
    player1_left: bool,
    player1_right: bool,
    player1_up: bool,
    player1_down: bool,
    player1_a: bool,
    player2_left: bool,
    player2_right: bool,
    player2_a: bool,
}

impl InputSnapshot {
    fn from_keyboard(state: &KeyboardState) -> Self {
        InputSnapshot {
            system_one_player: state.system_one_player,
            system_two_player: state.system_two_player,
            player1_left: state.player1_left,
            player1_right: state.player1_right,
            player1_up: state.player1_up,
            player1_down: state.player1_down,
            player1_a: state.player1_a,
            player2_left: state.player2_left,
            player2_right: state.player2_right,
            player2_a: state.player2_a,
        }
    }

    fn merge_controller(&mut self, controller: &ClassicController) {
        let ctrl = controller.state();
        self.system_one_player |= ctrl.system_one_player;
        self.system_two_player |= ctrl.system_two_player;
        self.player1_left |= ctrl.player1_left;
        self.player1_right |= ctrl.player1_right;
        self.player1_a |= ctrl.player1_a;
        self.player2_left |= ctrl.player2_left;
        self.player2_right |= ctrl.player2_right;
        self.player2_a |= ctrl.player2_a;
    }
}

impl GameState {
    fn new() -> Self {
        let mut state = GameState {
            players: Vec::new(),
            objects: Vec::new(),
            frame_count: 0,
            difficulty_multiplier: 1.0,
            spawn_meter: 0.0,
            controller: None,
            mode: PlayerMode::Single,
            phase: GamePhase::ModeSelect,
            menu_selection: PlayerMode::Single,
            last_system_one_player: false,
            last_system_two_player: false,
            last_confirm: false,
            last_up: false,
            last_down: false,
            last_left: false,
            last_right: false,
            final_scores: Vec::new(),
            leaderboard: Vec::new(),
            pending_scores: Vec::new(),
            current_name: String::new(),
            name_entry_index: 0,
        };
        state.load_leaderboard();
        state
    }

    fn set_controller(&mut self, controller: ClassicController) {
        self.controller = Some(controller);
    }

    fn reset_runtime(&mut self) {
        self.objects.clear();
        self.frame_count = 0;
        self.difficulty_multiplier = 1.0;
        self.spawn_meter = 0.0;
        self.final_scores.clear();
        self.pending_scores.clear();
        self.current_name.clear();
        self.name_entry_index = 0;
    }

    fn start_new_game(&mut self, mode: PlayerMode) {
        self.reset_runtime();
        self.mode = mode;
        self.players = (0..mode.player_count())
            .map(|idx| PlayerSlot::new(idx, mode.player_count()))
            .collect();
        self.phase = GamePhase::Playing;
    }

    fn back_to_menu(&mut self) {
        self.reset_runtime();
        self.players.clear();
        self.phase = GamePhase::ModeSelect;
        self.menu_selection = PlayerMode::Single;
        self.load_leaderboard(); // Refresh leaderboard when returning to menu
    }

    fn load_leaderboard(&mut self) {
        let window = web_sys::window().unwrap();
        if let Ok(Some(storage)) = window.local_storage() {
            if let Ok(Some(data)) = storage.get_item("black_friday_leaderboard") {
                if let Ok(parsed) = js_sys::JSON::parse(&data) {
                    let array = js_sys::Array::from(&parsed);
                    self.leaderboard.clear();
                    for i in 0..array.length() {
                        if let Some(entry) = array.get(i).dyn_ref::<js_sys::Object>() {
                            if let (Ok(score), Ok(mode_num)) = (
                                js_sys::Reflect::get(entry, &JsValue::from_str("score"))
                                    .and_then(|v| v.as_f64().ok_or(JsValue::NULL)),
                                js_sys::Reflect::get(entry, &JsValue::from_str("mode"))
                                    .and_then(|v| v.as_f64().ok_or(JsValue::NULL)),
                            ) {
                                let mode = if mode_num == 0.0 {
                                    PlayerMode::Single
                                } else {
                                    PlayerMode::Two
                                };
                                let name = js_sys::Reflect::get(entry, &JsValue::from_str("name"))
                                    .ok()
                                    .and_then(|v| v.as_string())
                                    .unwrap_or_else(|| "AAA".to_string());
                                self.leaderboard.push(LeaderboardEntry {
                                    score: score as i32,
                                    mode,
                                    name,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    fn save_leaderboard(&self) {
        let window = web_sys::window().unwrap();
        if let Ok(Some(storage)) = window.local_storage() {
            let array = js_sys::Array::new();
            for entry in &self.leaderboard {
                let obj = js_sys::Object::new();
                js_sys::Reflect::set(
                    &obj,
                    &JsValue::from_str("score"),
                    &JsValue::from_f64(entry.score as f64),
                )
                .unwrap();
                js_sys::Reflect::set(
                    &obj,
                    &JsValue::from_str("mode"),
                    &JsValue::from_f64(if entry.mode == PlayerMode::Single {
                        0.0
                    } else {
                        1.0
                    }),
                )
                .unwrap();
                js_sys::Reflect::set(
                    &obj,
                    &JsValue::from_str("name"),
                    &JsValue::from_str(&entry.name),
                )
                .unwrap();
                array.push(&obj);
            }
            if let Ok(json) = js_sys::JSON::stringify(&array) {
                let _ = storage.set_item("black_friday_leaderboard", &json.as_string().unwrap());
            }
        }
    }

    fn add_to_leaderboard(&mut self, score: i32, mode: PlayerMode, name: String) {
        self.leaderboard
            .push(LeaderboardEntry { score, mode, name });
        // Sort descending by score
        self.leaderboard.sort_by(|a, b| b.score.cmp(&a.score));
        // Keep only top 10
        if self.leaderboard.len() > 10 {
            self.leaderboard.truncate(10);
        }
        self.save_leaderboard();
    }

    fn start_name_entry(&mut self) {
        // Collect all scores that need names
        self.pending_scores = self.final_scores.clone();
        if self.pending_scores.is_empty() {
            // No scores to save, go straight to game over
            self.phase = GamePhase::GameOver;
            return;
        }
        self.name_entry_index = 0;
        self.current_name = String::from("AAA");
        self.phase = GamePhase::NameEntry;
    }

    fn handle_name_entry(&mut self, inputs: &InputSnapshot) {
        // Ensure name is 3 characters
        while self.current_name.len() < 3 {
            self.current_name.push('A');
        }
        let name_chars: Vec<char> = self.current_name.chars().take(3).collect();
        let mut name_chars: Vec<char> = name_chars.into_iter().collect();

        // Get current cursor position (0-2)
        let cursor_pos = (self.name_entry_index % 3).min(2);

        // Handle letter changes (up/down)
        if inputs.player1_up && !self.last_up {
            let current = name_chars.get(cursor_pos).copied().unwrap_or('A');
            let new_char = if current == 'A' {
                'Z'
            } else {
                char::from_u32(current as u32 - 1).unwrap_or('A')
            };
            if cursor_pos < name_chars.len() {
                name_chars[cursor_pos] = new_char;
            }
        }
        if inputs.player1_down && !self.last_down {
            let current = name_chars.get(cursor_pos).copied().unwrap_or('A');
            let new_char = if current == 'Z' {
                'A'
            } else {
                char::from_u32(current as u32 + 1).unwrap_or('Z')
            };
            if cursor_pos < name_chars.len() {
                name_chars[cursor_pos] = new_char;
            }
        }

        // Handle position changes (left/right)
        if inputs.player1_left && !self.last_left {
            if self.name_entry_index > 0 {
                self.name_entry_index -= 1;
            }
        }
        if inputs.player1_right && !self.last_right {
            if self.name_entry_index < 2 {
                self.name_entry_index += 1;
            }
        }

        // Update name
        self.current_name = name_chars.iter().take(3).collect();

        // Confirm name
        if inputs.player1_a {
            if let Some((player_index, score)) = self.pending_scores.first() {
                self.add_to_leaderboard(*score, self.mode, self.current_name.clone());
                self.pending_scores.remove(0);

                if self.pending_scores.is_empty() {
                    self.phase = GamePhase::GameOver;
                } else {
                    self.current_name = String::from("AAA");
                    self.name_entry_index = 0;
                }
            }
        }

        self.last_up = inputs.player1_up;
        self.last_down = inputs.player1_down;
        self.last_left = inputs.player1_left;
        self.last_right = inputs.player1_right;
    }

    fn update(&mut self) {
        if self.phase != GamePhase::Playing {
            return;
        }

        self.frame_count += 1;

        // Increase difficulty over time.
        //
        // We ramp up relatively quickly: every ~10 seconds at 60 FPS, we get a
        // noticeable bump in speed and spawn rate.
        if self.frame_count % 600 == 0 {
            self.difficulty_multiplier += 0.2;
        }

        // Spawn new objects based on a difficulty-scaled meter instead of fixed frames.
        //
        // Higher difficulty increases how fast the spawn meter fills, which means
        // more objects per second as you survive longer.
        let spawn_fill_rate = 1.0 * self.difficulty_multiplier;
        self.spawn_meter += spawn_fill_rate;

        let effective_interval = (BASE_SPAWN_INTERVAL / self.difficulty_multiplier).max(10.0); // cap so it never becomes *too* fast to be playable

        while self.spawn_meter >= effective_interval {
            self.spawn_meter -= effective_interval;
            self.spawn_object();

            // At very high difficulty, sometimes spawn an extra object for chaos.
            if self.difficulty_multiplier >= 2.0 {
                let mut rng = rand::thread_rng();
                if rng.gen_bool(0.25) {
                    self.spawn_object();
                }
            }
        }

        // Update falling objects
        let speed = OBJECT_SPEED * self.difficulty_multiplier;
        for obj in &mut self.objects {
            obj.y += speed;
        }

        // Check collisions
        self.check_collisions();

        // Remove objects that went off screen
        self.objects.retain(|obj| obj.y < CANVAS_HEIGHT);
    }

    fn spawn_object(&mut self) {
        let mut rng = rand::thread_rng();
        let x = rng.gen_range(0.0..CANVAS_WIDTH - OBJECT_WIDTH);

        // Base chance for a good deal goes down as difficulty increases,
        // so the game feels harsher the longer you survive.
        let mut good_chance = 0.6 - 0.15 * (self.difficulty_multiplier - 1.0);
        if good_chance < 0.25 {
            good_chance = 0.25;
        }

        let obj_type = if rng.gen_bool(good_chance) {
            ObjectType::GoodDeal
        } else {
            ObjectType::BadItem
        };

        self.objects.push(FallingObject {
            x,
            y: -OBJECT_HEIGHT,
            obj_type,
        });
    }

    fn check_collisions(&mut self) {
        if self.players.is_empty() {
            return;
        }

        let mut to_remove = Vec::new();

        for (i, obj) in self.objects.iter().enumerate() {
            let obj_left = obj.x;
            let obj_right = obj.x + OBJECT_WIDTH;
            let obj_top = obj.y;
            let obj_bottom = obj.y + OBJECT_HEIGHT;

            for player_slot in &mut self.players {
                // Skip dead players
                if player_slot.health <= 0 {
                    continue;
                }

                let player_left = player_slot.player.x;
                let player_right = player_slot.player.x + PLAYER_WIDTH;
                let player_top = player_slot.player.y;
                let player_bottom = player_slot.player.y + PLAYER_HEIGHT;

                if player_left < obj_right
                    && player_right > obj_left
                    && player_top < obj_bottom
                    && player_bottom > obj_top
                {
                    match obj.obj_type {
                        ObjectType::GoodDeal => {
                            player_slot.score += 10;
                        }
                        ObjectType::BadItem => {
                            player_slot.health -= 1;
                            if player_slot.health < 0 {
                                player_slot.health = 0;
                            }
                        }
                    }
                    to_remove.push(i);
                    break;
                }
            }
        }

        for &i in to_remove.iter().rev() {
            self.objects.remove(i);
        }

        // Store final scores and remove dead players
        let dead_players: Vec<_> = self
            .players
            .iter()
            .filter(|slot| slot.health <= 0)
            .map(|slot| (slot.player_index, slot.score))
            .collect();
        for (player_index, score) in dead_players {
            self.final_scores.push((player_index, score));
        }
        self.players.retain(|slot| slot.health > 0);

        // Game over when all players are dead
        if self.players.is_empty() && self.phase == GamePhase::Playing {
            self.start_name_entry();
        }
    }

    fn move_player(&mut self, player_index: usize, dx: f64) {
        // Find player by their original slot index (not array position)
        if let Some(player_slot) = self
            .players
            .iter_mut()
            .find(|slot| slot.player_index == player_index && slot.health > 0)
        {
            player_slot.player.x += dx * PLAYER_SPEED;
            if player_slot.player.x < 0.0 {
                player_slot.player.x = 0.0;
            }
            if player_slot.player.x > CANVAS_WIDTH - PLAYER_WIDTH {
                player_slot.player.x = CANVAS_WIDTH - PLAYER_WIDTH;
            }
        }
    }
}

fn setup_keyboard_listeners(state: Rc<RefCell<KeyboardState>>) -> Result<(), JsValue> {
    let window = web_sys::window().unwrap();

    {
        let state = state.clone();
        let keydown = Closure::wrap(Box::new(move |event: KeyboardEvent| {
            let mut state = state.borrow_mut();
            if state.handle_code(&event.code(), true) {
                event.prevent_default();
            }
        }) as Box<dyn FnMut(_)>);
        window.add_event_listener_with_callback("keydown", keydown.as_ref().unchecked_ref())?;
        keydown.forget();
    }

    {
        let state = state.clone();
        let keyup = Closure::wrap(Box::new(move |event: KeyboardEvent| {
            let mut state = state.borrow_mut();
            if state.handle_code(&event.code(), false) {
                event.prevent_default();
            }
        }) as Box<dyn FnMut(_)>);
        window.add_event_listener_with_callback("keyup", keyup.as_ref().unchecked_ref())?;
        keyup.forget();
    }

    Ok(())
}

fn draw(ctx: &CanvasRenderingContext2d, state: &GameState) {
    // Clear canvas
    ctx.set_fill_style(&JsValue::from_str("#111"));
    ctx.fill_rect(0.0, 0.0, CANVAS_WIDTH, CANVAS_HEIGHT);

    if state.phase == GamePhase::ModeSelect {
        ctx.set_fill_style(&JsValue::from_str("#fff"));
        ctx.set_font("18px monospace");
        ctx.fill_text("BLACK FRIDAY", 65.0, 80.0).unwrap();

        ctx.set_font("12px monospace");
        let single_prefix = if state.menu_selection == PlayerMode::Single {
            ">"
        } else {
            " "
        };
        let two_prefix = if state.menu_selection == PlayerMode::Two {
            ">"
        } else {
            " "
        };

        ctx.fill_text(&format!("{single_prefix} 1P – Solo shopper"), 60.0, 120.0)
            .unwrap();
        ctx.fill_text(&format!("{two_prefix} 2P – Shop with friend"), 60.0, 145.0)
            .unwrap();

        ctx.set_font("10px monospace");
        ctx.set_fill_style(&JsValue::from_str("#aaa"));
        ctx.fill_text("←/→: Select | A or 1P/2P: Start", 55.0, 175.0)
            .unwrap();
        ctx.fill_text("Catch $ deals, dodge red Xs", 70.0, 195.0)
            .unwrap();
        return;
    }

    if state.phase == GamePhase::NameEntry {
        ctx.set_fill_style(&JsValue::from_str("#fff"));
        ctx.set_font("14px monospace");

        if let Some((player_index, score)) = state.pending_scores.first() {
            ctx.fill_text(
                &format!("P{} SCORE: {}", player_index + 1, score),
                CANVAS_WIDTH / 2.0 - 60.0,
                50.0,
            )
            .unwrap();

            ctx.set_font("12px monospace");
            ctx.fill_text("ENTER NAME", CANVAS_WIDTH / 2.0 - 50.0, 80.0)
                .unwrap();

            // Draw name with cursor
            ctx.set_font("20px monospace");
            let name = if state.current_name.len() >= 3 {
                state.current_name.chars().take(3).collect::<String>()
            } else {
                format!("{:<3}", state.current_name)
            };

            let name_width = 60.0; // Approximate width for 3 chars
            let name_x = CANVAS_WIDTH / 2.0 - name_width / 2.0;
            let name_y = 120.0;

            // Draw each character with cursor indicator
            for (i, ch) in name.chars().enumerate() {
                let char_x = name_x + (i as f64 * 20.0);
                let is_cursor = i == (state.name_entry_index % 3);

                if is_cursor {
                    // Draw cursor line below
                    ctx.set_fill_style(&JsValue::from_str("#0ff"));
                    ctx.fill_rect(char_x, name_y + 20.0, 15.0, 2.0);
                }

                ctx.set_fill_style(&JsValue::from_str(if is_cursor { "#0ff" } else { "#fff" }));
                ctx.fill_text(&ch.to_string(), char_x, name_y).unwrap();
            }

            ctx.set_font("8px monospace");
            ctx.set_fill_style(&JsValue::from_str("#888"));
            ctx.fill_text("↑↓: Letter | ←→: Position", 50.0, 160.0)
                .unwrap();
            ctx.fill_text("A: Confirm", 120.0, 175.0).unwrap();
        }
        return;
    }

    if state.phase == GamePhase::GameOver {
        ctx.set_fill_style(&JsValue::from_str("#fff"));
        ctx.set_font("18px monospace");
        ctx.fill_text("GAME OVER", CANVAS_WIDTH / 2.0 - 50.0, 30.0)
            .unwrap();

        ctx.set_font("10px monospace");
        // Show current game scores
        let mut score_y = 55.0;
        for (player_index, score) in &state.final_scores {
            let text = format!("P{}: {}", player_index + 1, score);
            ctx.fill_text(&text, 10.0, score_y).unwrap();
            score_y += 12.0;
        }

        // Show leaderboard (top 5)
        ctx.set_font("9px monospace");
        ctx.set_fill_style(&JsValue::from_str("#aaa"));
        ctx.fill_text("TOP SCORES", 10.0, score_y + 5.0).unwrap();
        ctx.set_fill_style(&JsValue::from_str("#fff"));
        score_y += 18.0;

        for (i, entry) in state.leaderboard.iter().take(5).enumerate() {
            let mode_text = if entry.mode == PlayerMode::Single {
                "1P"
            } else {
                "2P"
            };
            let text = format!("{}. {} {} ({})", i + 1, entry.name, entry.score, mode_text);
            ctx.fill_text(&text, 10.0, score_y).unwrap();
            score_y += 11.0;
        }

        ctx.set_font("8px monospace");
        ctx.set_fill_style(&JsValue::from_str("#888"));
        ctx.fill_text("A: Menu | 1P/2P: Restart", 10.0, CANVAS_HEIGHT - 10.0)
            .unwrap();
        return;
    }

    let player_colors = ["#4a9eff", "#ff9f43"];

    for slot in &state.players {
        let color = player_colors.get(slot.player_index).unwrap_or(&"#4a9eff");
        ctx.set_fill_style(&JsValue::from_str(color));
        ctx.fill_rect(slot.player.x, slot.player.y, PLAYER_WIDTH, PLAYER_HEIGHT);
        ctx.set_stroke_style(&JsValue::from_str("#fff"));
        ctx.set_line_width(2.0);
        ctx.stroke_rect(slot.player.x, slot.player.y, PLAYER_WIDTH, PLAYER_HEIGHT);

        ctx.set_fill_style(&JsValue::from_str("#fff"));
        ctx.set_font("8px monospace");
        let label = format!("P{}", slot.player_index + 1);
        ctx.fill_text(&label, slot.player.x + 6.0, slot.player.y + 18.0)
            .unwrap();
    }

    // Draw falling objects
    for obj in &state.objects {
        match obj.obj_type {
            ObjectType::GoodDeal => {
                // Green for good deals
                ctx.set_fill_style(&JsValue::from_str("#00ff00"));
                ctx.fill_rect(obj.x, obj.y, OBJECT_WIDTH, OBJECT_HEIGHT);
                ctx.set_fill_style(&JsValue::from_str("#000"));
                ctx.set_font("14px monospace");
                ctx.fill_text("$", obj.x + 5.0, obj.y + 15.0).unwrap();
            }
            ObjectType::BadItem => {
                // Red for bad items
                ctx.set_fill_style(&JsValue::from_str("#ff0000"));
                ctx.fill_rect(obj.x, obj.y, OBJECT_WIDTH, OBJECT_HEIGHT);
                ctx.set_fill_style(&JsValue::from_str("#fff"));
                ctx.set_font("14px monospace");
                ctx.fill_text("X", obj.x + 5.0, obj.y + 15.0).unwrap();
            }
        }
    }

    // Draw HUD
    ctx.set_fill_style(&JsValue::from_str("#fff"));
    ctx.set_font("10px monospace");
    let mut hud_y = 15.0;
    for slot in &state.players {
        ctx.fill_text(
            &format!("P{} Score: {}", slot.player_index + 1, slot.score),
            5.0,
            hud_y,
        )
        .unwrap();

        let heart = "\u{2665}";
        ctx.set_fill_style(&JsValue::from_str("#ff4444"));
        for i in 0..slot.health {
            ctx.fill_text(
                heart,
                120.0 + (slot.player_index as f64 * 70.0) + (i as f64 * 12.0),
                hud_y,
            )
            .unwrap();
        }
        ctx.set_fill_style(&JsValue::from_str("#fff"));
        hud_y += 15.0;
    }

    // Draw instructions at the bottom
    ctx.set_fill_style(&JsValue::from_str("#888"));
    ctx.set_font("8px monospace");
    let instruction = if state.mode == PlayerMode::Two {
        "P1 & P2: D-Pads Move | $ = Good | X = Bad"
    } else {
        "D-Pad: Move | $ = Good | X = Bad"
    };
    ctx.fill_text(instruction, 40.0, CANVAS_HEIGHT - 5.0)
        .unwrap();
}

#[wasm_bindgen(start)]
pub fn main() -> Result<(), JsValue> {
    let window = web_sys::window().unwrap();
    let document = window.document().unwrap();
    let canvas = document.get_element_by_id("game").unwrap();
    let canvas: HtmlCanvasElement = canvas.dyn_into::<HtmlCanvasElement>()?;

    let context = canvas
        .get_context("2d")?
        .unwrap()
        .dyn_into::<CanvasRenderingContext2d>()?;

    let keyboard_state = Rc::new(RefCell::new(KeyboardState::default()));
    setup_keyboard_listeners(keyboard_state.clone())?;

    let game_state = Rc::new(RefCell::new(GameState::new()));

    // Acquire controller asynchronously
    let game_state_for_controller = game_state.clone();
    spawn_local(async move {
        if let Ok(controller) = ClassicController::acquire().await {
            game_state_for_controller
                .borrow_mut()
                .set_controller(controller);
        }
    });

    // Game loop
    let f = Rc::new(RefCell::new(None));
    let g = f.clone();

    let game_state_clone = game_state.clone();
    let keyboard_state_for_loop = keyboard_state.clone();
    *g.borrow_mut() = Some(Closure::wrap(Box::new(move || {
        let mut state = game_state_clone.borrow_mut();

        let mut inputs = {
            let keyboard_snapshot = keyboard_state_for_loop.borrow().clone();
            InputSnapshot::from_keyboard(&keyboard_snapshot)
        };
        if let Some(controller) = &state.controller {
            inputs.merge_controller(controller);
        }

        let confirm_now = inputs.player1_a || inputs.player2_a;
        let sys1_now = inputs.system_one_player;
        let sys2_now = inputs.system_two_player;

        match state.phase {
            GamePhase::ModeSelect => {
                // Menu navigation: left chooses 1P, right chooses 2P
                if inputs.player1_left || inputs.player2_left {
                    state.menu_selection = PlayerMode::Single;
                }
                if inputs.player1_right || inputs.player2_right {
                    state.menu_selection = PlayerMode::Two;
                }

                // System buttons instantly choose + start
                if sys2_now && !state.last_system_two_player {
                    state.start_new_game(PlayerMode::Two);
                } else if sys1_now && !state.last_system_one_player {
                    state.start_new_game(PlayerMode::Single);
                } else if confirm_now && !state.last_confirm {
                    // A starts currently highlighted option
                    let mode = state.menu_selection;
                    state.start_new_game(mode);
                }
            }
            GamePhase::GameOver => {
                if sys2_now && !state.last_system_two_player {
                    state.start_new_game(PlayerMode::Two);
                } else if sys1_now && !state.last_system_one_player {
                    state.start_new_game(PlayerMode::Single);
                } else if confirm_now && !state.last_confirm {
                    state.back_to_menu();
                }
            }
            GamePhase::Playing => {
                if inputs.player1_left {
                    state.move_player(0, -1.0);
                }
                if inputs.player1_right {
                    state.move_player(0, 1.0);
                }

                if state.mode == PlayerMode::Two {
                    if inputs.player2_left {
                        state.move_player(1, -1.0);
                    }
                    if inputs.player2_right {
                        state.move_player(1, 1.0);
                    }
                }
            }
            GamePhase::NameEntry => {
                state.handle_name_entry(&inputs);
            }
        }

        state.last_system_one_player = sys1_now;
        state.last_system_two_player = sys2_now;
        state.last_confirm = confirm_now;

        // Update game state
        state.update();

        // Draw
        draw(&context, &state);

        // Schedule next frame
        request_animation_frame(f.borrow().as_ref().unwrap());
    }) as Box<dyn FnMut()>));

    request_animation_frame(g.borrow().as_ref().unwrap());

    Ok(())
}

fn request_animation_frame(f: &Closure<dyn FnMut()>) {
    web_sys::window()
        .unwrap()
        .request_animation_frame(f.as_ref().unchecked_ref())
        .unwrap();
}
