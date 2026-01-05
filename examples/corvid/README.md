A CantaLoop module for using Bevy!

Why is it named **corvid**?

Corvids are a family of smart, adaptable birds—like crows and ravens—noted for their intelligence and problem-solving skills. The name reflects the goal of this module: to provide clever, flexible scripting for game worlds (and because it's a fun bird pun!). 

Also, because apperantly it eats melons.



This crate exposes [Bevy](https://bevyengine.org/) engine capabilities to Melon scripts via the CantaLoop VM.
It allows you to create and control 2D/3D games and simulations with ergonomic, scriptable logic.

## Getting Started

1. Add this module to your Melon project.
2. Import the module in your `.mln` file:
   ```
   mod bevy
   ```
3. Use the provided APIs to spawn entities, handle events, and drive your Bevy application from Melon code.

## Example

```melon
mod bevy

fn main() {
    bevy::startup()
    bevy::spawn_camera_2d()
    bevy::spawn_sprite("player.png", x = 100, y = 200)
}
```

## Features

- 2D entity spawning (`spawn_sprite`)
- Camera setup (`spawn_camera_2d`)
- Event handling (coming soon)
- Live reload (experimental)

## Requirements

- [Rust](https://rust-lang.org) and [Bevy](https://crates.io/crates/bevy) in your Rust project environment.
- CantaLoop VM (part of Melon)

## License

MIT
