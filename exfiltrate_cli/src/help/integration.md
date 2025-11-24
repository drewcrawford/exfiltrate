# Exfiltrate Integration Guide

This guide covers the basic steps to integrate the exfiltrate debugging framework into your Rust project.

## 1. Adding Exfiltrate as a Dependency

### For Library Targets

For library crates, it's recommended to place exfiltrate behind a feature flag to avoid including debugging code in production:

```toml
[features]
debug = ["exfiltrate"]

[dependencies]
exfiltrate = { version = "0.2.0", optional = true }
```

### For Binary Targets

For binary targets, you have two options:

**Option A: Feature Flag (Recommended)**
```toml
[features]
debug = ["exfiltrate", "your-lib/exfiltrate-feature"]

[dependencies]
exfiltrate = { version = "0.2.0", optional = true }
your-lib = { path = "../your-lib" }
```

**Option B: Debug-Only Compilation**
```toml
[dependencies]
exfiltrate = { version = "0.2.0", optional = true }
```

Then in your code:
```rust
#[cfg(debug_assertions)]
fn init_debug() {
 exfiltrate::begin();
}
```

**Important:** When using exfiltrate in a binary target, ensure you enable the relevant exfiltrate features in all downstream crates that you want to debug. This ensures you have access to all
debugging tools throughout your dependency tree.

## 1a. Enabling Optional Features

### Logwise Support

If your project uses the logwise logging framework, enable the logwise feature to capture and inspect logs remotely:

```toml
[dependencies]
exfiltrate = { version = "0.2.0", features = ["logwise"] }
```

With this feature enabled, exfiltrate will automatically:
- Capture all logwise log messages
- Provide a `logwise_logs` command to retrieve captured logs
- Allow remote inspection of your application's logging output

**Note:** Only enable the logwise feature if your project actually uses logwise. Enabling it unnecessarily will add an unused dependency to your build.

## 2. Calling the Startup Function

The `exfiltrate::begin()` function initializes the debugging server and must be called once at program startup.

### Binary Targets

In your `main.rs`, call the startup function as early as possible:

```rust
fn main() {
 #[cfg(feature="exfiltrate")] //or #[cfg(debug_assersions)] for Option B
 exfiltrate::begin();

 // Your application code here
 run_application();
}
```

### Library Targets

**General Rule:** Do NOT call `exfiltrate::begin()` in library code. Only the final binary should initialize exfiltrate to avoid conflicts.

**Exception for Debugging:** When debugging a specific issue where the binary doesn't call the startup function:

1. Temporarily add the startup call in your library for debugging:
```rust
#[cfg(feature = "debug")]
pub fn debug_init() {
   exfiltrate::begin();  // TEMPORARY - Remove before committing!
}
```

2. Debug your issue using the exfiltrate CLI

3. **Remove the startup call before committing your changes**

This approach ensures only one initialization point exists in production code while allowing flexible debugging during development.

## 3. Using Exfiltrate

Once integrated, your application will listen on `127.0.0.1:1337` for debugging connections. Use the exfiltrate CLI to interact with your running application:

```bash
# List available commands
exfiltrate list

# Get help for a specific command
exfiltrate help <command>

# View captured logs (if logwise feature is enabled)
exfiltrate logwise_logs
```

## Additional Resources

The exfiltrate framework supports advanced features like custom commands, image generation, and file transfers. To learn more about these capabilities, use the built-in help system:

```bash
exfiltrate help
```

This will provide detailed information about all available commands and features in your debugging session.