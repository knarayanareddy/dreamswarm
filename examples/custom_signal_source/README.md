# Example: Extending KAIROS Signal Sources 📡
The KAIROS background daemon uses signals to decide when to take initiative.

## Custom Signal Source
To add a new signal source, implement the `SignalSource` trait in `src/daemon/signals/`:

```rust
pub trait SignalSource: Send + Sync {
    fn poll(&mut self) -> Vec<Signal>;
    fn name(&self) -> &str;
}

pub struct MyCustomSignalSource;

impl SignalSource for MyCustomSignalSource {
    fn poll(&mut self) -> Vec<Signal> {
        // Your logic to detect events
        vec![Signal::Idle]
    }
    fn name(&self) -> &str { "MyCustomSignal" }
}
```

## Registration
Register in `SignalGatherer::with_defaults()` in `src/daemon/signals.rs`.

## Concepts
- **Signals**: Passive observations like file changes, git commits, or idle time.
- **Initiative**: The daemon's internal score (0.0 to 1.0) derived from gathered signals.
- **Trust**: The user-controlled safety gate for autonomous actions.
