# sdlrig

This is typicall how I run and compile the app:

```
RUST_BACKTRACE=1 RUSTFLAGS=-Zsanitizer=address cargo run --target aarch64-apple-darwin --bin=viz -- \
  --width=1440 --height=480 --fps 30 --hud-font=assets/VT323-Regular.ttf --hud-font-size=14 \
  --wasm=/Users/ttie/Desktop/veo/calc.wasm --preopen-dir=/Users/ttie/Desktop/tmp_viz
```

