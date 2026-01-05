use cantaloop::core::engine::{StdModule, StdStruct, StdFunction, Arity};
use cantaloop::core::hir_lowering::{ValueKind, FunctionSignature};
use cantaloop::core::vm::{Value, compute_struct_type_id};
use cantaloop::melon_module;
use bevy::prelude::*;
use bevy::window::WindowResolution;
use std::sync::Arc;

fn bevy_app_type_id() -> u32 {
    compute_struct_type_id("BevyApp")
}

// Helper function to create a BevyApp struct with given configuration
fn create_bevy_app(
    has_default_plugins: bool,
    window_title: String,
    window_width: f64,
    window_height: f64,
    has_clear_color: bool,
    clear_color_r: f64,
    clear_color_g: f64,
    clear_color_b: f64,
    heap: &mut cantaloop::core::vm::ValueHeap,
) -> Value {
    Value::struct_with_heap(
        bevy_app_type_id(),
        vec![
            Value::boolean(has_default_plugins),
            Value::string_with_heap(window_title, heap),
            Value::number(window_width),
            Value::number(window_height),
            Value::boolean(has_clear_color),
            Value::number(clear_color_r),
            Value::number(clear_color_g),
            Value::number(clear_color_b),
        ],
        heap,
    )
}

// Helper function to extract BevyApp fields
// The VM now forces thunks before passing arguments to native functions,
// so we can assume app is already evaluated
fn extract_bevy_app_fields(app: &Value, heap: &cantaloop::core::vm::ValueHeap) -> (bool, String, f64, f64, bool, f64, f64, f64) {
    let struct_data = app.as_struct(heap).expect(&format!("expected BevyApp struct, got: {:?}", app));
    let has_default_plugins = struct_data.fields[0].as_boolean().expect("expected boolean");
    let window_title = struct_data.fields[1].as_string(heap).expect("expected string").clone();
    let window_width = struct_data.fields[2].as_number().expect("expected number");
    let window_height = struct_data.fields[3].as_number().expect("expected number");
    let has_clear_color = struct_data.fields[4].as_boolean().expect("expected boolean");
    let clear_color_r = struct_data.fields[5].as_number().expect("expected number");
    let clear_color_g = struct_data.fields[6].as_number().expect("expected number");
    let clear_color_b = struct_data.fields[7].as_number().expect("expected number");
    (has_default_plugins, window_title, window_width, window_height, has_clear_color, clear_color_r, clear_color_g, clear_color_b)
}

lazy_static::lazy_static! {
    pub static ref BEVY_MODULE: StdModule = {
        // Start with the macro-generated module
        let mut module = melon_module! {
            module bevy {
            }
        };
        
        // Add app() function manually since melon_module! doesn't support struct return types
        module.functions.push(StdFunction {
            name: "app",
            signature: FunctionSignature {
                params: vec![],
                return_type: Box::new(ValueKind::Struct("BevyApp".into())),
            },
            arity: Arity::Fixed(0),
            impl_fn: Arc::new(|_args, heap| {
                create_bevy_app(
                    false, // has_default_plugins
                    String::new(), // window_title
                    0.0, // window_width
                    0.0, // window_height
                    false, // has_clear_color
                    0.0, // clear_color_r
                    0.0, // clear_color_g
                    0.0, // clear_color_b
                    heap,
                )
            }),
        });
        
        // Add the BevyApp struct definition
        module.structs.push(StdStruct {
            name: "BevyApp",
            fields: vec![
                ("has_default_plugins", ValueKind::Boolean),
                ("window_title", ValueKind::String),
                ("window_width", ValueKind::Number),
                ("window_height", ValueKind::Number),
                ("has_clear_color", ValueKind::Boolean),
                ("clear_color_r", ValueKind::Number),
                ("clear_color_g", ValueKind::Number),
                ("clear_color_b", ValueKind::Number),
            ],
            methods: vec![],
        });
        
        // Add builder functions
        module.functions.push(StdFunction {
            name: "with_default_plugins",
            signature: FunctionSignature {
                params: vec![ValueKind::Struct("BevyApp".into())],
                return_type: Box::new(ValueKind::Struct("BevyApp".into())),
            },
            arity: Arity::Fixed(1),
            impl_fn: Arc::new(|args, heap| {
                let (_, window_title, window_width, window_height, has_clear_color, clear_color_r, clear_color_g, clear_color_b) =
                    extract_bevy_app_fields(&args[0], heap);
                create_bevy_app(
                    true, // has_default_plugins = true
                    window_title,
                    window_width,
                    window_height,
                    has_clear_color,
                    clear_color_r,
                    clear_color_g,
                    clear_color_b,
                    heap,
                )
            }),
        });
        
        module.functions.push(StdFunction {
            name: "with_window",
            signature: FunctionSignature {
                params: vec![
                    ValueKind::Struct("BevyApp".into()),
                    ValueKind::String,
                    ValueKind::Number,
                    ValueKind::Number,
                ],
                return_type: Box::new(ValueKind::Struct("BevyApp".into())),
            },
            arity: Arity::Fixed(4),
            impl_fn: Arc::new(|args, heap| {
                let (has_default_plugins, _, _, _, has_clear_color, clear_color_r, clear_color_g, clear_color_b) =
                    extract_bevy_app_fields(&args[0], heap);
                let window_title = args[1].as_string(heap).expect("expected string").clone();
                let window_width = args[2].as_number().expect("expected number");
                let window_height = args[3].as_number().expect("expected number");
                create_bevy_app(
                    has_default_plugins,
                    window_title,
                    window_width,
                    window_height,
                    has_clear_color,
                    clear_color_r,
                    clear_color_g,
                    clear_color_b,
                    heap,
                )
            }),
        });
        
        module.functions.push(StdFunction {
            name: "with_clear_color",
            signature: FunctionSignature {
                params: vec![
                    ValueKind::Struct("BevyApp".into()),
                    ValueKind::Number,
                    ValueKind::Number,
                    ValueKind::Number,
                ],
                return_type: Box::new(ValueKind::Struct("BevyApp".into())),
            },
            arity: Arity::Fixed(4),
            impl_fn: Arc::new(|args, heap| {
                let (has_default_plugins, window_title, window_width, window_height, _, _, _, _) =
                    extract_bevy_app_fields(&args[0], heap);
                let clear_color_r = args[1].as_number().expect("expected number");
                let clear_color_g = args[2].as_number().expect("expected number");
                let clear_color_b = args[3].as_number().expect("expected number");
                create_bevy_app(
                    has_default_plugins,
                    window_title,
                    window_width,
                    window_height,
                    true, // has_clear_color = true
                    clear_color_r,
                    clear_color_g,
                    clear_color_b,
                    heap,
                )
            }),
        });
        
        // Effectful function: runs the Bevy app
        module.functions.push(StdFunction {
            name: "run",
            signature: FunctionSignature {
                params: vec![ValueKind::Struct("BevyApp".into())],
                return_type: Box::new(ValueKind::Void),
            },
            arity: Arity::Fixed(1),
            impl_fn: Arc::new(|args, heap| {
                let (has_default_plugins, window_title, window_width, window_height, has_clear_color, clear_color_r, clear_color_g, clear_color_b) =
                    extract_bevy_app_fields(&args[0], heap);
                
                // Build the Bevy App based on configuration
                let mut app = App::new();
                
                // Configure plugins and window
                let has_window_config = !window_title.is_empty() && window_width > 0.0 && window_height > 0.0;
                
                if has_default_plugins {
                    if has_window_config {
                        // Configure default plugins with custom window
                        app.add_plugins(DefaultPlugins.set(WindowPlugin {
                            primary_window: Some(Window {
                                title: window_title,
                                resolution: WindowResolution::new(window_width as u32, window_height as u32),
                                ..default()
                            }),
                            ..default()
                        }));
                    } else {
                        // Just use default plugins
                        app.add_plugins(DefaultPlugins);
                    }
                } else if has_window_config {
                    // Add only window plugin if default plugins are not enabled
                    app.add_plugins(MinimalPlugins.set(WindowPlugin {
                        primary_window: Some(Window {
                            title: window_title,
                            resolution: WindowResolution::new(window_width as u32, window_height as u32),
                            ..default()
                        }),
                        ..default()
                    }));
                }
                
                // Set clear color if explicitly configured
                if has_clear_color {
                    app.insert_resource(ClearColor(Color::srgb(
                        clear_color_r as f32,
                        clear_color_g as f32,
                        clear_color_b as f32,
                    )));
                }
                
                // Run the app (this blocks until the app exits)
                app.run();
                
                Value::none()
            }),
        });
        
        module
    };
}

#[no_mangle]
pub extern "C" fn register_native_modules(engine: *mut cantaloop::core::engine::Engine) {
    unsafe {
        if let Some(engine_ref) = engine.as_mut() {
            engine_ref.register_module(&*BEVY_MODULE, "");
        }
    }
}
