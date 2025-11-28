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
            health: 2,
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
}

#[derive(Default, Clone)]
struct KeyboardState {
    system_one_player: bool,
    system_two_player: bool,
    player1_left: bool,
    player1_right: bool,
    player1_a: bool,
    player2_left: bool,
    player2_right: bool,
    player2_a: bool,
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
        GameState {
            players: Vec::new(),
            objects: Vec::new(),
            frame_count: 0,
            difficulty_multiplier: 1.0,
            spawn_meter: 0.0,
            controller: None,
            mode: PlayerMode::Single,
            phase: GamePhase::ModeSelect,
        }
    }

    fn set_controller(&mut self, controller: ClassicController) {
        self.controller = Some(controller);
    }

    fn reset_runtime(&mut self) {
        self.objects.clear();
        self.frame_count = 0;
        self.difficulty_multiplier = 1.0;
        self.spawn_meter = 0.0;
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

        if self.players.iter().all(|slot| slot.health <= 0) {
            self.phase = GamePhase::GameOver;
        }
    }

    fn move_player(&mut self, index: usize, dx: f64) {
        if let Some(player_slot) = self.players.get_mut(index) {
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
        ctx.fill_text("BLACK FRIDAY", 65.0, 90.0).unwrap();
        ctx.set_font("12px monospace");
        ctx.fill_text("Press 1P to start solo", 80.0, 130.0)
            .unwrap();
        ctx.fill_text("Press 2P to team up", 78.0, 150.0).unwrap();
        ctx.set_font("10px monospace");
        ctx.set_fill_style(&JsValue::from_str("#aaa"));
        ctx.fill_text("Catch $ deals, dodge red Xs", 70.0, 190.0)
            .unwrap();
        return;
    }

    if state.phase == GamePhase::GameOver {
        ctx.set_fill_style(&JsValue::from_str("#fff"));
        ctx.set_font("20px monospace");
        ctx.fill_text("GAME OVER", CANVAS_WIDTH / 2.0 - 55.0, 90.0)
            .unwrap();
        ctx.set_font("12px monospace");

        for (idx, slot) in state.players.iter().enumerate() {
            let text = format!("P{} Score: {}", idx + 1, slot.score);
            ctx.fill_text(&text, 85.0, 130.0 + (idx as f64 * 18.0))
                .unwrap();
        }

        ctx.set_font("10px monospace");
        ctx.fill_text("Press A to return to menu", 60.0, 190.0)
            .unwrap();
        ctx.fill_text("Or choose 1P/2P to start again", 40.0, 210.0)
            .unwrap();
        return;
    }

    let player_colors = ["#4a9eff", "#ff9f43"];

    for (idx, slot) in state.players.iter().enumerate() {
        let color = player_colors.get(idx).unwrap_or(&"#4a9eff");
        ctx.set_fill_style(&JsValue::from_str(color));
        ctx.fill_rect(slot.player.x, slot.player.y, PLAYER_WIDTH, PLAYER_HEIGHT);
        ctx.set_stroke_style(&JsValue::from_str("#fff"));
        ctx.set_line_width(2.0);
        ctx.stroke_rect(slot.player.x, slot.player.y, PLAYER_WIDTH, PLAYER_HEIGHT);

        ctx.set_fill_style(&JsValue::from_str("#fff"));
        ctx.set_font("8px monospace");
        let label = format!("P{}", idx + 1);
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
    for (idx, slot) in state.players.iter().enumerate() {
        let y = 15.0 + (idx as f64 * 15.0);
        ctx.fill_text(&format!("P{} Score: {}", idx + 1, slot.score), 5.0, y)
            .unwrap();

        let heart = "\u{2665}";
        ctx.set_fill_style(&JsValue::from_str("#ff4444"));
        for i in 0..slot.health {
            ctx.fill_text(heart, 120.0 + (idx as f64 * 70.0) + (i as f64 * 12.0), y)
                .unwrap();
        }
        ctx.set_fill_style(&JsValue::from_str("#fff"));
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

        match state.phase {
            GamePhase::ModeSelect => {
                if inputs.system_two_player
                    || inputs.player2_left
                    || inputs.player2_right
                    || inputs.player2_a
                {
                    state.start_new_game(PlayerMode::Two);
                } else if inputs.system_one_player
                    || inputs.player1_left
                    || inputs.player1_right
                    || inputs.player1_a
                {
                    state.start_new_game(PlayerMode::Single);
                }
            }
            GamePhase::GameOver => {
                if inputs.system_two_player {
                    state.start_new_game(PlayerMode::Two);
                } else if inputs.system_one_player {
                    state.start_new_game(PlayerMode::Single);
                } else if inputs.player1_a || inputs.player2_a {
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
        }

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
