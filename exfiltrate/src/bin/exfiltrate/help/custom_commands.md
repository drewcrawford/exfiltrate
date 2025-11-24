# Writing Custom Commands

This guide continues after the integration walkthrough and explains how to extend Exfiltrate with your own remote-debug commands.

## Command Syntax

Commands implement the [`exfiltrate::command::Command`](../../../src/lib.rs) trait (definition in `exfiltrate_internal/src/command.rs`). Each command must:

- Return a unique `name()` so that `exfiltrate help <name>` can find it.
- Provide `short_description()` and `full_description()` strings.
- Implement `execute(&self, args: Vec<String>) -> Result<Response, Response>`.

A minimal command looks like:

```rust
use exfiltrate::command::{Command, Response};

struct HelloWorld;
impl Command for HelloWorld {
    fn name(&self) -> &'static str { "hello_world" }
    fn short_description(&self) -> &'static str {
        "Returns a hello world message. Use this to test the Exfiltrate wiring."
    }
    fn full_description(&self) -> &'static str {
        "Returns a hello world message.\nUsage: exfiltrate hello_world"
    }
    fn execute(&self, _args: Vec<String>) -> Result<Response, Response> {
        Ok("hello world".into())
    }
}
```

## Response Formats

The `Response` enum supports three variants for returning different types of data from your commands:

### String Response

The most common format for returning text data. Strings implement `Into<Response>` for convenience:

```rust
fn execute(&self, _args: Vec<String>) -> Result<Response, Response> {
    Ok("hello world".into())
}
```

For structured data, use `Response::from_serialize`:

```rust
fn execute(&self, _args: Vec<String>) -> Result<Response, Response> {
    let data = MySerializableStruct { /* ... */ };
    Response::from_serialize(&data)
}
```

### Files Response

Use `Response::Files(Vec<FileInfo>)` to return one or more binary files that should be saved to disk. The CLI will automatically save each file with a randomly generated filename and the suggested extension:

```rust
use exfiltrate::command::{Response, FileInfo};

fn execute(&self, _args: Vec<String>) -> Result<Response, Response> {
    // Single file - automatically wrapped in a vector via .into()
    let binary_data: Vec<u8> = vec![/* ... */];
    let file = FileInfo::new(
        "log".to_string(),              // proposed extension
        Some("Debug logs".to_string()), // optional remark shown to user
        binary_data                     // file contents
    );
    Ok(file.into())
}
```

For multiple files:

```rust
fn execute(&self, _args: Vec<String>) -> Result<Response, Response> {
    let files = vec![
        FileInfo::new("txt".to_string(), Some("Config".to_string()), config_data),
        FileInfo::new("json".to_string(), Some("State".to_string()), state_data),
        FileInfo::new("log".to_string(), None, log_data),
    ];
    Ok(files.into())
}
```

### Images Response

Use `Response::Images(Vec<ImageInfo>)` to return one or more RGBA images that the CLI can save as image files. The image data must be in RGBA8 format (8 bits per channel: red, green, blue, alpha) using the `exfiltrate::rgb::RGBA8` type:

```rust
use exfiltrate::command::{Response, ImageInfo};
use exfiltrate::rgb::RGBA8;

fn execute(&self, _args: Vec<String>) -> Result<Response, Response> {
    // Single image - automatically wrapped in a vector via .into()
    let width: u32 = 255;
    let height: u32 = 255;
    let mut data: Vec<RGBA8> = Vec::with_capacity((width * height) as usize);

    // Generate image data
    for r in 0..255u8 {
        for g in 0..255u8 {
            let b = 255u8.saturating_sub(r / 2).saturating_sub(g / 2);
            data.push(rgb::RGBA { r, g, b, a: 255 });
        }
    }

    Ok(ImageInfo::new(
        data,
        width,
        Some("Generated test pattern".to_string()) // optional remark
    ).into())
}
```

For multiple images:

```rust
fn execute(&self, _args: Vec<String>) -> Result<Response, Response> {
    let images = vec![
        ImageInfo::new(frame1_data, width, Some("Frame 1".to_string())),
        ImageInfo::new(frame2_data, width, Some("Frame 2".to_string())),
        ImageInfo::new(frame3_data, width, Some("Frame 3".to_string())),
    ];
    Ok(images.into())
}
```

For complete examples, see `examples/debug.rs` which demonstrates string, single image, multiple images, and multiple files responses.

### Common Pitfalls

**Thread-safe access patterns**: If your command needs to coordinate with other threads
(e.g., render loops), use appropriate synchronization primitives.

#### Feature Flag Best Practices

When integrating exfiltrate commands deeply with your library's internals (like render loops, game engines, or async runtimes), you'll need to gate exfiltrate-specific code with `#[cfg(feature =
  "exfiltrate")]`. 
**Gate minimally** - only the code that actually uses exfiltrate types.

### Don't Gate Entire Functions

**❌ WRONG - Gating the entire function:**
  ```rust
#[cfg(feature = "exfiltrate")]
fn setup_debug_capture(&self, ...) -> Option<DebugData> {
    // Entire function only exists with feature enabled
    // This creates duplicate declarations and inconsistent APIs
}
```

✅ CORRECT - Gate only the implementation:
```rust
fn setup_debug_capture(&self, ...) -> Option<DebugData> {
    if self.capture_sender.is_some() {
        #[cfg(feature = "exfiltrate")] {
            // Only the exfiltrate-specific implementation is gated
            let data = self.create_capture_data();
            Some(data)
        }
        #[cfg(not(feature = "exfiltrate"))] {
            unreachable!("capture_sender should only be Some with exfiltrate feature")
        }
    } else {
        None
    }
}
```

Why this matters:
- Keeps your API surface consistent regardless of feature flags
- Prevents duplicate function declarations
- Makes it clear which parts actually depend on exfiltrate
- Allows types like DebugData to be generic/reusable

#### Gate State Fields, Not Functions

Only add #[cfg(feature = "exfiltrate")] to struct fields that store exfiltrate-specific state:

```rust
pub struct RenderState {
    pub frame_count: u32,
    pub surface: Surface,

    // Only this field is exfiltrate-specific
    #[cfg(feature = "exfiltrate")]
    pub capture_sender: Option<Sender<ImageInfo>>,
}
```

Then in your functions, check the field with cfg only when accessing it:

```rust
fn update_state(&mut self) {
    self.frame_count += 1;

    #[cfg(feature = "exfiltrate")]
    if let Some(sender) = self.capture_sender.take() {
        // Send capture data
    }
}
```

## Important: Type Imports

**NEVER add `exfiltrate-internal` as a dependency in your Cargo.toml.** This is a private implementation crate and is not part of the public API.

All types you need are re-exported through the public `exfiltrate` crate:

```rust
// ✅ CORRECT - Import from the public exfiltrate crate
use exfiltrate::command::{Command, Response, ImageInfo, FileInfo};
use exfiltrate::rgb::RGBA8;
```

```rust
// ❌ WRONG - Never import from exfiltrate_internal
use exfiltrate_internal::command::{ImageInfo, FileInfo};  // DO NOT DO THIS
```

```toml
# ❌ WRONG - Never add exfiltrate-internal to Cargo.toml
[dependencies]
exfiltrate-internal = { ... }  # DO NOT DO THIS
```

**Why this matters**: `exfiltrate_internal` is an internal implementation detail that may change without notice. Depending on it directly will:
- Break your code when internal APIs change
- Create maintenance burden
- Violate proper dependency boundaries

If you find a type is not available from `exfiltrate::command` or `exfiltrate::rgb`, please report it as a bug - it should be re-exported.

## Register Commands Immediately

### For Binary Crates

Call `exfiltrate::begin()` as soon as your binary starts and register commands immediately afterward with [`exfiltrate::add_command`](../../../src/lib.rs#L19):

```rust
fn main() {
    #[cfg(feature = "exfiltrate")]
    {
        exfiltrate::begin();
        exfiltrate::add_command(HelloWorld);
        // Additional commands...
    }

    // Your application code...
}
```

`add_command` stores commands in a global list read by the built-in `list` and `help` handlers, so adding them in your startup path guarantees the CLI can discover them before a remote session attaches.

### For Library Crates

**IMPORTANT**: Library crates must register their commands within the library itself, NOT in examples or consuming binaries.

1. Create a module in your library for exfiltrate commands (e.g., `src/exfiltrate_commands.rs`):

```rust
// src/exfiltrate_commands.rs
use exfiltrate::command::{Command, Response};

pub(crate) struct MyLibraryCommand;
impl Command for MyLibraryCommand {
    fn name(&self) -> &'static str { "my_library_command" }
    fn short_description(&self) -> &'static str {
        "Debugging command for MyLibrary. Use this to inspect internal state."
    }
    fn full_description(&self) -> &'static str {
        "Detailed help for my_library_command..."
    }
    fn execute(&self, _args: Vec<String>) -> Result<Response, Response> {
        Ok("Command output".into())
    }
}

/// Registers all exfiltrate commands for this library.
/// This is called automatically during library initialization.
pub(crate) fn register_commands() {
    exfiltrate::add_command(MyLibraryCommand);
    // Register additional library commands...
}
```

2. Add the module to your `src/lib.rs`:

```rust
// src/lib.rs
#[cfg(feature = "exfiltrate")]
mod exfiltrate_commands;  // Note: NOT pub, just mod

// Your library code...
```

3. Call `register_commands()` from a common library initialization point. For example, when creating your main library struct:

```rust
pub struct MyLibrary {
    // ...
}

impl MyLibrary {
    pub fn new() -> Self {
        #[cfg(feature = "exfiltrate")]
        crate::exfiltrate_commands::register_commands();

        Self { /* ... */ }
    }
}
```

Or use a `lazy_static` / `once_cell` for registration if you don't have a natural initialization point:

```rust
#[cfg(feature = "exfiltrate")]
use std::sync::Once;

#[cfg(feature = "exfiltrate")]
static REGISTER_COMMANDS: Once = Once::new();

pub fn ensure_exfiltrate_registered() {
    #[cfg(feature = "exfiltrate")]
    REGISTER_COMMANDS.call_once(|| {
        crate::exfiltrate_commands::register_commands();
    });
}

// Call this from your library's commonly-used entry points
```

4. In examples, ONLY call `exfiltrate::begin()`:

```rust
// examples/my_example.rs
fn main() {
    #[cfg(feature = "exfiltrate")]
    exfiltrate::begin();

    // Your example code that uses the library...
    let lib = MyLibrary::new(); // Commands are registered here
}
```

**Why this pattern matters**: Examples are consumers of your library. Commands should be registered by the library itself so they're available to all consumers, not just specific examples. If you register commands in examples, they won't be available when others use your library.

## Crafting Short Descriptions

`short_description()` is what `exfiltrate list` displays (`src/commands/list.rs`). Keep it to a few sentences at most:

1. Start with the overall action (e.g., “Captures the current matchmaking queue state.”).
2. Follow with a “Use this to…” clause that clearly states when engineers should reach for the command.

Concise copy makes the listing readable even when many commands are registered.

## Writing Full Descriptions

`full_description()` is shown by `exfiltrate help <name>` (`src/commands/help.rs`). Treat it as a living runbook:

- Cover every argument, expected output, and return shape.
- Call out advanced use cases, related commands, and troubleshooting tips (for instance, “If the response is `Err`, inspect the service logs for …”).
- Include multi-line text if needed; newlines are preserved in the CLI output.

A thorough full description keeps remote debugging self-serve and avoids context switches back to source control or chat.***
