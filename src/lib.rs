use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement};
use rand::Rng;
use std::cell::RefCell;
use std::rc::Rc;

const CANVAS_WIDTH: f64 = 336.0;
const CANVAS_HEIGHT: f64 = 262.0;
const PLAYER_WIDTH: f64 = 30.0;
const PLAYER_HEIGHT: f64 = 30.0;
const PLAYER_SPEED: f64 = 3.0;
const OBJECT_WIDTH: f64 = 20.0;
const OBJECT_HEIGHT: f64 = 20.0;
const OBJECT_SPEED: f64 = 2.0;
const SPAWN_INTERVAL: u32 = 60; // frames

#[derive(Clone)]
enum ObjectType {
    GoodDeal,  // Catch these for points
    BadItem,   // Dodge these or lose health
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

struct GameState {
    player: Player,
    objects: Vec<FallingObject>,
    score: i32,
    health: i32,
    frame_count: u32,
    game_over: bool,
    difficulty_multiplier: f64,
}

impl GameState {
    fn new() -> Self {
        GameState {
            player: Player {
                x: CANVAS_WIDTH / 2.0 - PLAYER_WIDTH / 2.0,
                y: CANVAS_HEIGHT - PLAYER_HEIGHT - 20.0,
            },
            objects: Vec::new(),
            score: 0,
            health: 3,
            frame_count: 0,
            game_over: false,
            difficulty_multiplier: 1.0,
        }
    }

    fn update(&mut self) {
        if self.game_over {
            return;
        }

        self.frame_count += 1;

        // Increase difficulty over time
        if self.frame_count % 600 == 0 {
            self.difficulty_multiplier += 0.1;
        }

        // Spawn new objects
        if self.frame_count % SPAWN_INTERVAL == 0 {
            self.spawn_object();
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

        // Check game over
        if self.health <= 0 {
            self.game_over = true;
        }
    }

    fn spawn_object(&mut self) {
        let mut rng = rand::thread_rng();
        let x = rng.gen_range(0.0..CANVAS_WIDTH - OBJECT_WIDTH);

        // 60% good deals, 40% bad items
        let obj_type = if rng.gen_bool(0.6) {
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
        let player_left = self.player.x;
        let player_right = self.player.x + PLAYER_WIDTH;
        let player_top = self.player.y;
        let player_bottom = self.player.y + PLAYER_HEIGHT;

        let mut to_remove = Vec::new();

        for (i, obj) in self.objects.iter().enumerate() {
            let obj_left = obj.x;
            let obj_right = obj.x + OBJECT_WIDTH;
            let obj_top = obj.y;
            let obj_bottom = obj.y + OBJECT_HEIGHT;

            // Check if rectangles overlap
            if player_left < obj_right
                && player_right > obj_left
                && player_top < obj_bottom
                && player_bottom > obj_top
            {
                match obj.obj_type {
                    ObjectType::GoodDeal => {
                        self.score += 10;
                    }
                    ObjectType::BadItem => {
                        self.health -= 1;
                    }
                }
                to_remove.push(i);
            }
        }

        // Remove collected/hit objects (in reverse to maintain indices)
        for &i in to_remove.iter().rev() {
            self.objects.remove(i);
        }
    }

    fn move_player(&mut self, dx: f64) {
        self.player.x += dx * PLAYER_SPEED;

        // Keep player in bounds
        if self.player.x < 0.0 {
            self.player.x = 0.0;
        }
        if self.player.x > CANVAS_WIDTH - PLAYER_WIDTH {
            self.player.x = CANVAS_WIDTH - PLAYER_WIDTH;
        }
    }

    fn reset(&mut self) {
        *self = GameState::new();
    }
}

fn draw(ctx: &CanvasRenderingContext2d, state: &GameState) {
    // Clear canvas
    ctx.set_fill_style(&JsValue::from_str("#111"));
    ctx.fill_rect(0.0, 0.0, CANVAS_WIDTH, CANVAS_HEIGHT);

    if state.game_over {
        // Draw game over screen
        ctx.set_fill_style(&JsValue::from_str("#fff"));
        ctx.set_font("20px monospace");
        ctx.fill_text("GAME OVER", CANVAS_WIDTH / 2.0 - 55.0, CANVAS_HEIGHT / 2.0 - 20.0).unwrap();

        ctx.set_font("12px monospace");
        let score_text = format!("Score: {}", state.score);
        ctx.fill_text(&score_text, CANVAS_WIDTH / 2.0 - 30.0, CANVAS_HEIGHT / 2.0).unwrap();

        ctx.set_font("10px monospace");
        ctx.fill_text("Press A to Restart", CANVAS_WIDTH / 2.0 - 45.0, CANVAS_HEIGHT / 2.0 + 20.0).unwrap();
        return;
    }

    // Draw player (shopping cart)
    ctx.set_fill_style(&JsValue::from_str("#4a9eff"));
    ctx.fill_rect(state.player.x, state.player.y, PLAYER_WIDTH, PLAYER_HEIGHT);
    ctx.set_stroke_style(&JsValue::from_str("#fff"));
    ctx.set_line_width(2.0);
    ctx.stroke_rect(state.player.x, state.player.y, PLAYER_WIDTH, PLAYER_HEIGHT);

    // Draw cart label
    ctx.set_fill_style(&JsValue::from_str("#fff"));
    ctx.set_font("8px monospace");
    ctx.fill_text("CART", state.player.x + 4.0, state.player.y + 18.0).unwrap();

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
    ctx.fill_text(&format!("Score: {}", state.score), 5.0, 15.0).unwrap();

    // Draw health hearts
    let heart = "\u{2665}"; // ♥
    ctx.set_fill_style(&JsValue::from_str("#ff0000"));
    ctx.set_font("10px monospace");
    for i in 0..state.health {
        ctx.fill_text(heart, 5.0 + (i as f64 * 15.0), 30.0).unwrap();
    }

    // Draw instructions at the bottom
    ctx.set_fill_style(&JsValue::from_str("#888"));
    ctx.set_font("8px monospace");
    ctx.fill_text("D-PAD: Move | $ = Good | X = Bad", 60.0, CANVAS_HEIGHT - 5.0).unwrap();
}

#[wasm_bindgen(module = "/node_modules/@rcade/plugin-input-classic/dist/index.js")]
extern "C" {
    #[wasm_bindgen(js_name = PLAYER_1)]
    static PLAYER_1: JsValue;

    #[wasm_bindgen(js_name = SYSTEM)]
    static SYSTEM: JsValue;
}

fn get_input_value(obj: &JsValue, key: &str) -> bool {
    unsafe {
        js_sys::Reflect::get(obj, &JsValue::from_str(key))
            .ok()
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    }
}

fn get_dpad_value(obj: &JsValue, direction: &str) -> bool {
    unsafe {
        js_sys::Reflect::get(obj, &JsValue::from_str("DPAD"))
            .ok()
            .and_then(|dpad| js_sys::Reflect::get(&dpad, &JsValue::from_str(direction)).ok())
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    }
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

    let game_state = Rc::new(RefCell::new(GameState::new()));

    // Game loop
    let f = Rc::new(RefCell::new(None));
    let g = f.clone();

    let game_state_clone = game_state.clone();
    *g.borrow_mut() = Some(Closure::wrap(Box::new(move || {
        let mut state = game_state_clone.borrow_mut();

        // Handle input
        unsafe {
            if state.game_over {
                // Check for restart
                if get_input_value(&PLAYER_1, "A") {
                    state.reset();
                }
            } else {
                // Player movement
                if get_dpad_value(&PLAYER_1, "left") {
                    state.move_player(-1.0);
                }
                if get_dpad_value(&PLAYER_1, "right") {
                    state.move_player(1.0);
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
